namespace TrailBase;

/// <summary>
/// Error representing fetch errors.
/// </summary>
public class FetchException : Exception {
  /// <summary>Auth subject, i.e. user id.</summary>
  public System.Net.HttpStatusCode Status { get; }

  /// <summary>
  /// FetchException constructor.
  /// </summary>
  /// <param name="status">HTTP status code.</param>
  /// <param name="message">Error message</param>
  public FetchException(System.Net.HttpStatusCode status, string message) : base(message) {
    this.Status = status;
  }

  /// <summary>Stringify FetchException.</summary>
  public override string ToString() {
    return $"FetchException(status={Status}, '{Message}')";
  }
}
