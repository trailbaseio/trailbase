import { initClient } from "trailbase";

const client = initClient("http://localhost:4000");
await client.login("admin@localhost", "secret");

const movies = client.records("movies");
const m = await movies.list({
  pagination: {
    limit: 3,
  },
  order: ["rank"],
  filters: [
    {
      column: "watch_time",
      op: "lessThan",
      value: "120",
    },
  ],
});

console.log(m.records);
