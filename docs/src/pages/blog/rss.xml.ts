import type { APIContext } from "astro";
import { getCollection } from "astro:content";
import rss from "@astrojs/rss";

import config from "@/config";

export async function GET(_context: APIContext) {
  const blog = await getCollection("blog");

  return rss({
    title: config.title,
    description: config.description,
    site: config.site,
    items: blog.map((post) => ({
      title: post.data.title,
      pubDate: post.data.pubDate,
      description: post.data.intro,
      link: `/blog/${post.slug}/`,
    })),
    customData: `<language>en-us</language>`,
  });
}
