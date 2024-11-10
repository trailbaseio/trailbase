import { addRoute } from "trailbase:main";

type Request = {
  uri: string,
  headers: unknown,
  body: Uint8Array,
};

addRoute("GET", "/test", (req: Request) => {
  console.log("Request", req);
  return "js response";
});
