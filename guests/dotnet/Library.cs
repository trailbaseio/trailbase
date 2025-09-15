// Reference: https://github.com/bytecodealliance/componentize-dotnet/tree/main/test//WasmComponentSdkTest/testapps/OciWit
using System.Text;

namespace TrailbaseWorld.wit.exports.trailbase.runtime
{
    public class InitEndpointImpl : IInitEndpoint
    {
        public static IInitEndpoint.InitResult Init()
        {
            Console.WriteLine("HERE");
            throw new Exception("test");
            return new IInitEndpoint.InitResult(httpHandlers: [], jobHandlers: []);
        }
    }
}

namespace ProxyWorld.wit.exports.wasi.http.v0_2_0
{
    using ProxyWorld.wit.imports.wasi.http.v0_2_0;

    public class IncomingHandlerImpl : IIncomingHandler
    {
        public static void Handle(ITypes.IncomingRequest request, ITypes.ResponseOutparam responseOut)
        {
            var content = Encoding.ASCII.GetBytes("Hello, from C#!");
            var headers = new List<(string, byte[])> {
                ("content-type", Encoding.ASCII.GetBytes("text/plain")),
                ("content-length", Encoding.ASCII.GetBytes(content.Count().ToString()))
            };

            var response = new ITypes.OutgoingResponse(ITypes.Fields.FromList(headers));
            var body = response.Body();
            ITypes.ResponseOutparam.Set(responseOut, Result<ITypes.OutgoingResponse, ITypes.ErrorCode>.Ok(response));
            using (var stream = body.Write())
            {
                stream.BlockingWriteAndFlush(content);
            }
            ITypes.OutgoingBody.Finish(body, null);
        }
    }
}
