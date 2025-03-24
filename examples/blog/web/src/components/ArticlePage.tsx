import { createResource, Match, Switch } from "solid-js";
import { useStore } from "@nanostores/solid";

import { $client } from "@/lib/client";
import { imageUrl } from "@/lib/article";

import { Tag } from "@/components/Tag";
import { PublishDate } from "@/components/PublishDate";

import type { Article } from "@schema/article";

function ArticlePageImpl(props: { article: Article }) {
  return (
    <article class="mx-auto w-full px-7 py-10">
      <div class="prose mx-auto lg:prose-xl prose-headings:mb-3 prose-headings:font-bold prose-headings:text-pacamara-dark">
        <h1 class="transition-all duration-300 dark:text-white">
          {props.article.title}
        </h1>

        <p class="mx-auto mb-7 flex max-w-screen-lg flex-row flex-wrap items-center gap-5 font-pacamara-space">
          <Tag tag={props.article.tag} />
          <PublishDate date={props.article.created} />
          <span class="mb-1">
            {props.article.username ?? props.article.author}
          </span>
        </p>
      </div>

      <img
        src={imageUrl(props.article)}
        width="1200"
        height="250"
        alt=""
        loading="eager"
        decoding="sync"
        class="image-shine relative mx-auto mt-10 block h-72 rounded-[15px] object-cover md:h-[350px]"
      />

      <div class={articleContentStyle}>{props.article.body}</div>
    </article>
  );
}

export function ArticlePage() {
  const client = useStore($client);

  const urlParams = new URLSearchParams(window.location.search);
  const articleId = urlParams.get("id");
  if (!articleId) {
    throw Error("missing article id query parameter");
  }

  const [article] = createResource(client, async (client) => {
    console.log("Before");
    const x = await client?.records("articles_view").read<Article>(articleId);
    console.log("After", x);
    return x;
  });

  return (
    <Switch>
      <Match when={article.error}>Failed to load: {`${article.error}`}</Match>
      <Match when={article.loading}>Loading... {article.state}</Match>

      <Match when={article()}>
        <ArticlePageImpl article={article()!} />
      </Match>
    </Switch>
  );
}

const articleContentStyle =
  "lg:px-0 pt-10 mb-5 mx-auto prose lg:prose-xl prose-headings:transition-all prose-headings:duration-300 prose-headings:font-pacamara-space prose-headings:font-bold prose-headings:text-pacamara-accent prose-headings:mb-0 prose-headings:pb-3 prose-headings:mt-6 prose-p:transition-all prose-p:duration-300 prose-p:text-pacamara-primary/80 prose-li:transition-all prose-li:duration-300 prose-li:text-pacamara-primary/80 prose-td:transition-all prose-td:duration-300 prose-td:text-pacamara-primary/80 prose-a:underline prose-a:font-semibold prose-a:transition-all prose-a:duration-300 prose-a:text-pacamara-accent hover:prose-a:text-pacamara-dark prose-strong:transition-all prose-strong:duration-300 prose-strong:font-bold prose-hr:transition-all prose-hr:duration-300 prose-hr:border-pacamara-secondary/40 prose-img:rounded-lg prose-img:mx-auto prose-code:transition-all prose-code:duration-300 prose-code:text-pacamara-dark dark:prose-headings:text-pacamara-accent dark:prose-p:text-white/70 dark:prose-a:text-white dark:hover:prose-a:text-pacamara-accent dark:prose-strong:text-white dark:prose-li:text-white dark:prose-code:text-white dark:prose-td:text-white/70 dark:prose-hr:border-pacamara-accent/30 dark:prose-tr:border-pacamara-accent/30 dark:prose-thead:border-pacamara-accent/30";
