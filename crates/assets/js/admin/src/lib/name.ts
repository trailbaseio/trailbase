import { v4 } from "uuid";

const adjectives = [
  "adorable",
  "casual",
  "dapper",
  "feral",
  "fresh",
  "friendly",
  "funky",
  "funny",
  "gnarly",
  "hasty",
  "intrepid",
  "jumpy",
  "lengthy",
  "lucid",
  "snappy",
  "sublime",
  "swift",
];

const nouns = [
  "ant",
  "badger",
  "bee",
  "canine",
  "cobra",
  "fink",
  "fox",
  "iguana",
  "koala",
  "lion",
  "lizard",
  "lynx",
  "otter",
  "owl",
  "panda",
  "possum",
  "raccoon",
  "tiger",
  "wombat",
];

export function generateRandomName(opts: { taken?: string[] }): string {
  const taken = opts?.taken ?? [];

  const candidate = () => {
    const prefix = adjectives[getRandomInt(adjectives.length - 1)];
    const suffix = nouns[getRandomInt(nouns.length - 1)];
    return `${prefix}_${suffix}`;
  };

  for (let i = 0; i < 10; ++i) {
    const name = candidate();
    if (taken.findIndex((n) => n === name) === -1) {
      return name;
    }
  }

  return v4();
}

function getRandomInt(max: number): number {
  return Math.floor(Math.random() * max);
}
