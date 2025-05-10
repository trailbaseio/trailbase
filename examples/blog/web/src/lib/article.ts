import { filePath } from "trailbase";

import { $client } from "@/lib/client";
import type { Article } from "@schema/article";

export function imageUrl(article: Article): string {
  const client = $client.get();
  if (client && article.image) {
    const path = filePath("articles_view", article.id, "image");
    return import.meta.env.DEV
      ? new URL(path, "http://localhost:4000").toString()
      : path;
  }
  return "/default.svg";
}

export const buttonStyle =
  "h-10 px-4 py-2 border border-input bg-pacamara-secondary hover:text-pacamara-accent foreground inline-flex items-center justify-center rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50";
export const inputStyle =
  "flex h-10 w-full rounded-md border border-input px-3 py-2 text-sm ring-offset-background file:border-0 file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50";
