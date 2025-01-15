using Microsoft.Extensions.Logging;
using System.IdentityModel.Tokens.Jwt;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json.Serialization;
using System.Text.Json;

namespace TrailBase;

/// <summary>
/// Representation of User JSON objects.
/// </summary>
public class User {
  /// <summary>Auth subject, i.e. user id.</summary>
  public string sub { get; }
  /// <summary>The user's email address.</summary>
  public string email { get; }

  /// <summary>
  /// User constructor.
  /// </summary>
  /// <param name="sub">Auth subject, i.e. user id.</param>
  /// <param name="email">User's email address</param>
  public User(string sub, string email) {
    this.sub = sub;
    this.email = email;
  }
}

/// <summary>
/// Representation of Credentials JSON objects used for log in.
/// </summary>
public class Credentials {
  /// <summary>The user's email address.</summary>
  public string email { get; }
  /// <summary>The user's password.</summary>
  public string password { get; }

  /// <summary>
  /// Credentials constructor.
  /// </summary>
  /// <param name="email">User's email address</param>
  /// <param name="password">User's password</param>
  public Credentials(string email, string password) {
    this.email = email;
    this.password = password;
  }
}

/// <summary>
/// Representation of RefreshTokenRequest JSON objects.
/// </summary>
public class RefreshTokenRequest {
  /// <summary>The refresh token received at login.</summary>
  public string refresh_token { get; }

  /// <summary>
  /// RefreshTokenRequest constructor.
  /// </summary>
  /// <param name="refreshToken">The refresh token.</param>
  public RefreshTokenRequest(string refreshToken) {
    refresh_token = refreshToken;
  }
}

/// <summary>
/// Representation of RefreshTokenResponse JSON objects provided on refresh.
/// </summary>
public class RefreshTokenResponse {
  /// <summary>New auth token in exchange for the refresh token.</summary>
  public string auth_token { get; }
  /// <summary>Cross-site request forgery token.</summary>
  public string? csrf_token { get; }

  /// <summary>
  /// RefreshTokenResponse constructor.
  /// </summary>
  /// <param name="authToken">User authentication token.</param>
  /// <param name="csrfToken">User Cross-site request forgery token.</param>
  public RefreshTokenResponse(string authToken, string? csrfToken) {
    auth_token = authToken;
    csrf_token = csrfToken;
  }
}

/// <summary>
/// Representation of Token JSON objects provided on login.
/// </summary>
public class Tokens {
  /// <summary>User auth token.</summary>
  public string auth_token { get; }
  /// <summary>User refresh token for future auth token exchanges.</summary>
  public string? refresh_token { get; }
  /// <summary>Cross-site request forgery token.</summary>
  public string? csrf_token { get; }

  /// <summary>
  /// Tokens constructor.
  /// </summary>
  public Tokens(string auth_token, string? refresh_token, string? csrf_token) {
    this.auth_token = auth_token;
    this.refresh_token = refresh_token;
    this.csrf_token = csrf_token;
  }

  /// <summary>Serialize Tokens.</summary>
  public override string ToString() {
    return $"Tokens({auth_token}, {refresh_token}, {csrf_token})";
  }
}

/// <summary>
/// Representation of JwtToken JSON objects.
/// </summary>
public class JwtToken {
  /// <summary>Auth subject, i.e. user id.</summary>
  public string sub { get; }
  /// <summary>JWT token issue timestamp.</summary>
  public long iat { get; }
  /// <summary>Expiration timestamp.</summary>
  public long exp { get; }
  /// <summary>User's email address.</summary>
  public string email { get; }
  /// <summary>Cross-site request forgery token.</summary>
  public string csrf_token { get; }

  /// <summary>JwtToken constructor.</summary>
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
[JsonSerializable(typeof(RefreshTokenResponse))]
[JsonSerializable(typeof(RefreshTokenRequest))]
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
        (
          tokens,
          JsonSerializer.Deserialize<JwtToken>(json, SourceGenerationContext.Default.JwtToken)
        )!,
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

internal class ThinClient {
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

/// <summary>
/// The main API for interacting with TrailBase servers.
/// </summary>
public class Client {
  static readonly string _authApi = "api/auth/v1";
  static readonly ILogger logger = LoggerFactory.Create(
      builder => builder.AddConsole()).CreateLogger("TrailBase.Client");

  ThinClient client;
  /// <summary>Site this Client is connected to.</summary>
  public string site { get; }
  TokenState tokenState;

  /// <summary>
  /// Construct a TrailBase Client for the given [site] and [tokens].
  /// </summary>
  /// <param name="site">Site URL, e.g. https://trailbase.io:4000.</param>
  /// <param name="tokens">Optional tokens for a given authenticated user</param>
  public Client(String site, Tokens? tokens) {
    client = new ThinClient(site);
    this.site = site;
    tokenState = TokenState.build(tokens);
  }

  /// <summary>Get tokens of the logged-in user if any.</summary>
  public Tokens? Tokens() => tokenState.state?.Item1;
  /// <summary>Get the logged-in user if any.</summary>
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

  /// <summary>Construct a record API object for the API with the given name.</summary>
  public RecordApi Records(string name) {
    return new RecordApi(this, name);
  }

  /// <summary>Log in with the given credentials.</summary>
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

  /// <summary>Log out the current user.</summary>
  public async Task<bool> Logout() {
    var refreshToken = tokenState.state?.Item1.refresh_token;

    try {
      if (refreshToken != null) {
        var tokenJson = JsonContent.Create(
            new RefreshTokenRequest(refreshToken),
            SourceGenerationContext.Default.RefreshTokenRequest
        );
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

  /// <summary>Refresh the current auth token.</summary>
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
      JsonContent.Create(
        new RefreshTokenRequest(refreshToken),
        SourceGenerationContext.Default.RefreshTokenRequest
      ),
      HttpMethod.Post,
      null
    );

    string json = await response.Content.ReadAsStringAsync();
    RefreshTokenResponse tokenResponse = JsonSerializer.Deserialize<RefreshTokenResponse>(
        json,
        SourceGenerationContext.Default.RefreshTokenResponse
    )!;

    return TokenState.build(new Tokens(
      tokenResponse.auth_token,
      refreshToken,
      tokenResponse.csrf_token
    ));
  }

  internal async Task<HttpResponseMessage> Fetch(
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
