import { defineCollection, reference, z } from "astro:content";
import { docsSchema } from "@astrojs/starlight/schema";

const docsCollection = defineCollection({ schema: docsSchema() });

const blogCollection = defineCollection({
  type: "content",
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
