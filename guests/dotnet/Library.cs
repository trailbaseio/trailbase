// Reference: https://github.com/bytecodealliance/componentize-dotnet/tree/main/test//WasmComponentSdkTest/testapps/OciWit
using System.Text;

public static class Utilities
{
    public static int fibonacci(int num)
    {
        switch (num)
        {
            case 0:
                return 0;
            case 1:
                return 1;
            default:
                return fibonacci(num - 1) + fibonacci(num - 2);
        }
    }
}

namespace TrailbaseWorld.wit.exports.trailbase.runtime
{
    public class InitEndpointImpl : IInitEndpoint
    {
        public static IInitEndpoint.InitResult Init()
        {
            Console.WriteLine("InitEndpoint.Init");
            return new IInitEndpoint.InitResult(httpHandlers: [(IInitEndpoint.MethodType.GET, "/fibonacci")], jobHandlers: []);
        }
    }
}

namespace ProxyWorld.wit.exports.wasi.http.v0_2_0
{
    using ProxyWorld.wit.imports.wasi.http.v0_2_0;
    using static ProxyWorld.wit.imports.wasi.http.v0_2_0.ITypes;

    public class IncomingHandlerImpl : IIncomingHandler
    {


        public static void Handle(IncomingRequest request, ResponseOutparam responseOut)
        {
            Console.WriteLine($"http.IncomingHandler.Handle");

            var fib = Utilities.fibonacci(40);
            var content = Encoding.ASCII.GetBytes($"{fib}\n");

            var headers = new List<(string, byte[])> {
                ("content-type", Encoding.ASCII.GetBytes("text/plain")),
                ("content-length", Encoding.ASCII.GetBytes(content.Count().ToString()))
            };

            SendResponse(responseOut, headers, content);
        }

        public static void SendResponse(ResponseOutparam responseOut, List<(string, byte[])> headers, byte[] bodyBytes)
        {
            // FIXME: Needed due to a bug in WIT.bindgen for dotnet.
            // https://github.com/bytecodealliance/wit-bindgen/pull/1215
            var responseHeaders = Fields.FromList(new List<(string, byte[])>());
            try
            {
                responseHeaders = Fields.FromList(headers);
            }
            catch (Exception)
            {
                Console.WriteLine("WARN: dotnet header conversion still broken");
            }

            var response = new OutgoingResponse(responseHeaders);
            response.SetStatusCode(200);
            var body = response.Body();

            ResponseOutparam.Set(responseOut, Result<OutgoingResponse, ErrorCode>.Ok(response));
            using (var stream = body.Write())
            {
                stream.BlockingWriteAndFlush(bodyBytes);
            }

            OutgoingBody.Finish(body, null);
        }
    }
}
