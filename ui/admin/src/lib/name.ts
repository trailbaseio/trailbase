function getRandomInt(max: number): number {
  return Math.floor(Math.random() * max);
}

const adjectives = [
  "lucid",
  "feral",
  "jumpy",
  "hasty",
  "gnarly",
  "friendly",
  "fresh",
  "funny",
  "lengthy",
];

const nouns = [
  "lynx",
  "badger",
  "lion",
  "panda",
  "ant",
  "fink",
  "lizard",
  "canine",
  "tiger",
];

export function randomName(): string {
  const prefix = adjectives[getRandomInt(adjectives.length - 1)];
  const suffix = nouns[getRandomInt(nouns.length - 1)];
  return `${prefix}_${suffix}`;
}
