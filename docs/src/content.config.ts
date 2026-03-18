import { defineCollection, reference } from "astro:content";
import { z } from "astro/zod";
import { docsSchema } from "@astrojs/starlight/schema";
import { glob } from "astro/loaders";

const docsCollection = defineCollection({
  loader: glob({ pattern: "**/[^_]*.{md,mdx}", base: "./src/content/docs" }),
  schema: docsSchema(),
});

const blogCollection = defineCollection({
  loader: glob({ pattern: "**/[^_]*.{md,mdx}", base: "./src/data/blog" }),
  schema: ({ image }) =>
    z.object({
      title: z.string(),
      intro: z.string(),
      tags: z.array(z.string()),
      image: image().optional(),
      author: reference("author"),
      pubDate: z.date(),
      type: z.string().optional(),
    }),
});

const authorCollection = defineCollection({
  type: "data",
  schema: ({ image }) =>
    z.object({
      displayName: z.string(),
      bio: z.string().optional(),
      photo: image().optional(),
    }),
});

export const collections = {
  docs: docsCollection,
  blog: blogCollection,
  author: authorCollection,
};
