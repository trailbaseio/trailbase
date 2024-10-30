import { readFile } from "node:fs/promises";
import { parse } from "csv-parse/sync";

import { Client } from "trailbase";
import type { Movie } from "@schema/movie";

const client = new Client("http://localhost:4000");
await client.login("admin@localhost", "secret");
const api = client.records("movies");

let movies = [];
do {
  movies = await api.list<Movie>({
    pagination: {
      limit: 100,
    },
  });

  for (const movie of movies) {
    await api.delete(movie.rank!);
  }
} while (movies.length > 0);

const file = await readFile("../data/Top_1000_IMDb_movies_New_version.csv");
const records = parse(file, {
  fromLine: 2,
  // prettier-ignore
  columns: [ "rank", "name", "year", "watch_time", "rating", "metascore", "gross", "votes", "description" ],
});

for (const movie of records) {
  await api.create<Movie>({
    rank: parseInt(movie.rank),
    name: movie.name,
    year: movie.year,
    watch_time: parseInt(movie.watch_time),
    rating: parseInt(movie.rating),
    metascore: movie.metascore,
    gross: movie.gross,
    votes: movie.votes,
    description: movie.description,
  });
}
