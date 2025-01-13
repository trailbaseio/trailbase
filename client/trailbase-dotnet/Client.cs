using Microsoft.Extensions.Logging;
using System.IdentityModel.Tokens.Jwt;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json.Serialization;
using System.Text.Json;

namespace TrailBase;

public class User {
  public string sub { get; }
  public string email { get; }

  public User(string sub, string email) {
    this.sub = sub;
    this.email = email;
  }
}

public class Credentials {
  public string email { get; }
  public string password { get; }

  public Credentials(string email, string password) {
    this.email = email;
    this.password = password;
  }
}

public class RefreshToken {
  public string refresh_token { get; }

  public RefreshToken(string refreshToken) {
    refresh_token = refreshToken;
  }
}

public class TokenResponse {
  public string auth_token { get; }
  public string? csrf_token { get; }

  public TokenResponse(string authToken, string? csrfToken) {
    auth_token = authToken;
    csrf_token = csrfToken;
  }
}

public class Tokens {
  public string auth_token { get; }
  public string? refresh_token { get; }
  public string? csrf_token { get; }

  public Tokens(string auth_token, string? refresh_token, string? csrf_token) {
    this.auth_token = auth_token;
    this.refresh_token = refresh_token;
    this.csrf_token = csrf_token;
  }

  public override string ToString() {
    return $"Tokens({auth_token}, {refresh_token}, {csrf_token})";
  }
}

public class JwtToken {
  public string sub { get; }
  public long iat { get; }
  public long exp { get; }
  public string email { get; }
  public string csrf_token { get; }

  [JsonConstructor]
  public JwtToken(
    string sub,
    long iat,
    long exp,
    string email,
    string csrf_token
  ) {
    this.sub = sub;
    this.iat = iat;
    this.exp = exp;
    this.email = email;
    this.csrf_token = csrf_token;
  }
}

[JsonSourceGenerationOptions(WriteIndented = true)]
[JsonSerializable(typeof(Credentials))]
[JsonSerializable(typeof(JwtToken))]
[JsonSerializable(typeof(Tokens))]
[JsonSerializable(typeof(TokenResponse))]
[JsonSerializable(typeof(RefreshToken))]
[JsonSerializable(typeof(User))]
internal partial class SourceGenerationContext : JsonSerializerContext {
}

class TokenState {
  public (Tokens, JwtToken)? state;
  public HttpRequestHeaders headers;

  TokenState((Tokens, JwtToken)? state, HttpRequestHeaders headers) {
    this.state = state;
    this.headers = headers;
  }

  public static TokenState build(Tokens? tokens) {
    var authToken = tokens?.auth_token;
    if (authToken != null) {
      var handler = new JwtSecurityTokenHandler();
      var jwtToken = (JwtSecurityToken)handler.ReadToken(authToken);
      var json = jwtToken.Payload.SerializeToJson();

      return new TokenState(
        (tokens, JsonSerializer.Deserialize<JwtToken>(json, SourceGenerationContext.Default.JwtToken))!,
        buildHeaders(tokens)
      );
    }
    return new TokenState(null, buildHeaders(tokens));
  }

  static HttpRequestHeaders buildHeaders(Tokens? tokens) {
    var headers = new HttpRequestMessage().Headers;

    if (tokens != null) {
      headers.Add("Authorization", $"Bearer {tokens.auth_token}");

      var refresh = tokens.refresh_token;
      if (refresh != null) {
        headers.Add("Refresh-Token", refresh);
      }

      var csrf = tokens.csrf_token;
      if (csrf != null) {
        headers.Add("CSRF-Token", csrf);
      }
    }

    return headers;
  }
}

class ThinClient {
  static readonly HttpClient client = new HttpClient();

  string site;

  internal ThinClient(string site) {
    this.site = site;
  }

  internal async Task<HttpResponseMessage> Fetch(
    String path,
    TokenState tokenState,
    HttpContent? data,
    HttpMethod? method,
    Dictionary<string, string>? queryParams,
    HttpCompletionOption completion = HttpCompletionOption.ResponseContentRead
  ) {
    if (path.StartsWith('/')) {
      throw new ArgumentException("Path starts with '/'. Relative path expected.");
    }

    var query = (Dictionary<string, string> p) => {
      var queryString = System.Web.HttpUtility.ParseQueryString(string.Empty);
      foreach (var e in p) {
        queryString.Add(e.Key, e.Value);
      }
      return queryString.ToString();
    };

    var httpRequestMessage = new HttpRequestMessage {
      Method = method ?? HttpMethod.Post,
      RequestUri =
        queryParams switch {
          null => new Uri($"{site}/{path}"),
          _ => new Uri($"{site}/{path}?{query(queryParams)}"),
        },
      Content = data,
    };

    foreach (var (key, values) in tokenState.headers) {
      foreach (var value in values) {
        httpRequestMessage.Headers.Add(key, value);
      }
    }

    return await client.SendAsync(httpRequestMessage, completion);
  }
}

public class Client {
  static readonly string _authApi = "api/auth/v1";
  static readonly ILogger logger = LoggerFactory.Create(builder => builder.AddConsole()).CreateLogger("TrailBase.Client");

  ThinClient client;
  public string site { get; }
  TokenState tokenState;

  public Client(String site, Tokens? tokens) {
    client = new ThinClient(site);
    this.site = site;
    tokenState = TokenState.build(tokens);
  }

  public Tokens? Tokens() => tokenState.state?.Item1;
  public User? User() {
    var authToken = Tokens()?.auth_token;
    if (authToken != null) {
      var handler = new JwtSecurityTokenHandler();
      var jwtToken = (JwtSecurityToken)handler.ReadToken(authToken);
      var json = jwtToken.Payload.SerializeToJson();

      return JsonSerializer.Deserialize<User>(json, SourceGenerationContext.Default.User);
    }
    return null;
  }

  public RecordApi Records(string name) {
    return new RecordApi(this, name);
  }

  public async Task<Tokens> Login(string email, string password) {
    var response = await Fetch(
      $"{_authApi}/login",
      HttpMethod.Post,
      JsonContent.Create(new Credentials(email, password), SourceGenerationContext.Default.Credentials),
      null
    );

    string json = await response.Content.ReadAsStringAsync();
    Tokens tokens = JsonSerializer.Deserialize<Tokens>(json, SourceGenerationContext.Default.Tokens)!;
    updateTokens(tokens);
    return tokens;
  }

  public async Task<bool> Logout() {
    var refreshToken = tokenState.state?.Item1.refresh_token;

    try {
      if (refreshToken != null) {
        var tokenJson = JsonContent.Create(new RefreshToken(refreshToken), SourceGenerationContext.Default.RefreshToken);
        await Fetch($"{_authApi}/logout", HttpMethod.Post, tokenJson, null);
      }
      else {
        await Fetch($"{_authApi}/logout", null, null, null);
      }
    }
    catch (Exception err) {
      logger.LogWarning($"{err}");
    }
    updateTokens(null);
    return true;
  }

  static string? shouldRefresh(TokenState tokenState) {
    var now = DateTimeOffset.Now.ToUnixTimeSeconds();
    var state = tokenState.state;
    if (state != null) {
      if (state.Value.Item2.exp - 60 < now) {
        return state.Value.Item1.refresh_token;
      }
    }
    return null;
  }

  TokenState updateTokens(Tokens? tokens) {
    var ts = TokenState.build(tokens);

    tokenState = ts;
    // _authChange?.call(this, state.state?.$1);

    var claims = ts.state?.Item2;
    if (claims != null) {
      var now = DateTimeOffset.Now.ToUnixTimeSeconds();
      if (claims.exp < now) {
        logger.LogWarning("Token expired");
      }
    }

    return ts;
  }

  public async Task RefreshAuthToken() {
    var refreshToken = shouldRefresh(tokenState);
    if (refreshToken != null) {
      tokenState = await refreshTokensImpl(refreshToken);
    }
  }

  async Task<TokenState> refreshTokensImpl(string refreshToken) {
    var response = await client.Fetch(
      $"{_authApi}/refresh",
      tokenState,
      JsonContent.Create(new RefreshToken(refreshToken), SourceGenerationContext.Default.RefreshToken),
      HttpMethod.Post,
      null
    );

    string json = await response.Content.ReadAsStringAsync();
    TokenResponse tokenResponse = JsonSerializer.Deserialize<TokenResponse>(json, SourceGenerationContext.Default.TokenResponse)!;

    return TokenState.build(new Tokens(
      tokenResponse.auth_token,
      refreshToken,
      tokenResponse.csrf_token
    ));
  }

  public async Task<HttpResponseMessage> Fetch(
    string path,
    HttpMethod? method,
    HttpContent? data,
    Dictionary<string, string>? queryParams,
    HttpCompletionOption completion = HttpCompletionOption.ResponseContentRead
  ) {
    var ts = tokenState;
    var refreshToken = shouldRefresh(tokenState);
    if (refreshToken != null) {
      ts = tokenState = await refreshTokensImpl(refreshToken);
    }

    var response = await client.Fetch(path, ts, data, method, queryParams, completion);

    if (response.StatusCode != System.Net.HttpStatusCode.OK) {
      string errMsg = await response.Content.ReadAsStringAsync();
      throw new Exception($"Fetch failed [{response.StatusCode}]: {errMsg}");
    }

    return response;
  }
}
