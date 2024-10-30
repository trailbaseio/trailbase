import { Client } from "trailbase";

const client = new Client("http://localhost:4000");
await client.login("admin@localhost", "secret");

const movies = client.records("movies");
const m = await movies.list({
  pagination: {
    limit: 3,
  },
  order: ["rank"],
  filters: ["watch_time[lt]=120"],
});

console.log(m);
