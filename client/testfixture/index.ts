import { addRoute } from "trailbase:main";

type Headers = { [key: string]: string };
type Request = {
  uri: string;
  headers: Headers;
  body: string;
};
type Response = {
  headers?: Headers;
  status?: number;
  body?: string;
};

addRoute("GET", "/test", (req: Request) : Response => {
  console.log("Request", req);
  return {
    body: "js response",
  };
});
