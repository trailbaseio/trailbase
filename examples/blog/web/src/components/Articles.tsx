import {
  createResource,
  createSignal,
  For,
  Match,
  Show,
  Switch,
} from "solid-js";
import { useStore } from "@nanostores/solid";
import { TbPencilPlus } from "solid-icons/tb";
import { Client } from "trailbase";

import { $client } from "@/lib/client";
import { $profile, createProfile } from "@/lib/profile";

import type { Article } from "@schema/article";

function PublishDate(props: { date: number }) {
  return (
    <span class="text-pacamara-primary/50 font-pacamara-space transition-all duration-300 text-[16px] dark:text-white/40">
      {new Date(props.date * 1000).toLocaleDateString()}
    </span>
  );
}

function Tag(props: { tag: string }) {
  const style =
    "text-[16px] border-pacamara-secondary border-[1px] leading-none rounded-full flex items-center h-[34px] px-2 text-pacamara-secondary";
  return (
    <div class="flex items-center gap-2">
      <For each={props.tag.split(",").map((t) => t.trim())}>
        {(tag) => <span class={style}>{tag}</span>}
      </For>
    </div>
  );
}

export function ComposeArticleButton() {
  const profile = useStore($profile);

  return (
    <Show when={profile()?.profile?.is_editor ?? false} fallback={<></>}>
      <a href="/compose">
        <TbPencilPlus class="p-1 size-6 bg-pacamara-secondary inline-block rounded-full dark:text-white" />
      </a>
    </Show>
  );
}

function ArticleCard(props: { article: Article; index: number }) {
  const article = props.article;
  const isOdd = props.index % 2;

  const link = `/article/?id=${article.id}`;
  const classList =
    "rounded-[15px] image-shine object-cover h-[200px] " +
    (isOdd ? "rotate-2" : "-rotate-2");

  return (
    <article class={articleCardStyle}>
      <div class="grid grid-cols-1 lg:grid-cols-[200px_auto] md:grid-cols-[200px_auto] gap-10 items-center">
        <div>
          <a href={link}>
            <img
              src={imageUrl(article)}
              width="750"
              alt={article.title + "Thumbnail"}
              class={classList}
            />
          </a>
        </div>

        <div>
          <h2>
            <a
              href={link}
              class="text-pacamara-dark hover:text-pacamara-accent"
            >
              {article.title}
            </a>
          </h2>

          <p>{article.intro}</p>

          <p class="flex flex-row flex-wrap gap-4 items-center mt-5 group-last:mb-0">
            <Tag tag={article.tag} />
            <PublishDate date={article.created} />
            <span>by {article.username}</span>
          </p>
        </div>
      </div>
    </article>
  );
}

function AssignNewUsername(props: { client: Client }) {
  const [error, setError] = createSignal<unknown | undefined>();
  const [username, setUsername] = createSignal("");

  return (
    <div>
      <h1 class="text-xl font-black">Pick a unique username:</h1>

      <div class="flex gap-4 py-4">
        <input
          class={inputStyle}
          type="text"
          placeholder="username"
          onKeyUp={(e) => setUsername(e.currentTarget.value)}
        />

        <button
          class={buttonStyle}
          onClick={async () => {
            try {
              await createProfile(props.client, username());
              window.location.reload();
            } catch (err) {
              setError(err);
            }
          }}
        >
          Submit
        </button>
      </div>

      {error() !== undefined && (
        <strong class="text-xl text-red-700">{`${error()}`}</strong>
      )}
    </div>
  );
}

export function ArticleList() {
  const client = useStore($client);
  const [articles] = createResource(client, (client) =>
    client?.records("articles_view").list<Article>(),
  );
  const profile = useStore($profile);

  return (
    <Switch fallback={<p>Loading...</p>}>
      <Match when={articles.error}>{`${articles.error}`}</Match>

      <Match when={client() && articles() && profile()?.missingProfile}>
        <AssignNewUsername client={client()!} />
      </Match>

      <Match when={articles()}>
        <For each={articles()}>
          {(item, index) => <ArticleCard article={item} index={index()} />}
        </For>
      </Match>
    </Switch>
  );
}

function ArticlePageImpl(props: { article: Article }) {
  const article = () => props.article;

  return (
    <article class="px-7 py-10 mx-auto w-full">
      <div class="mx-auto prose lg:prose-xl prose-headings:font-bold prose-headings:text-pacamara-dark prose-headings:mb-3">
        <h1 class="transition-all duration-300 dark:text-white">
          {article().title}
        </h1>

        <p class="flex flex-row flex-wrap gap-5 items-center font-pacamara-space mx-auto max-w-screen-lg mb-7">
          <Tag tag={article().tag} />
          <PublishDate date={article().created} />
          <span class="mb-1">{props.article.username ?? article().author}</span>
        </p>
      </div>

      <img
        src={imageUrl(article())}
        width="1200"
        height="250"
        alt=""
        loading="eager"
        decoding="sync"
        class="block relative mx-auto mt-10 object-cover h-72 md:h-[350px] image-shine rounded-[15px]"
      />

      <div class={articleContentStyle}>{article().body}</div>
    </article>
  );
}

export function ArticlePage() {
  const client = useStore($client);

  const urlParams = new URLSearchParams(window.location.search);
  const articleId = urlParams.get("id");
  if (!articleId) {
    throw "missing article id query parameter";
  }

  const [article] = createResource(client, (client) =>
    client?.records("articles_view").read<Article>(articleId),
  );

  return (
    <Switch fallback={<p>Loading...</p>}>
      <Match when={article.error}>Failed to load: {`${article.error}`}</Match>

      <Match when={article()}>
        <ArticlePageImpl article={article()!} />
      </Match>
    </Switch>
  );
}

function imageUrl(article: Article): string {
  const client = $client.get();
  if (client && article.image) {
    return client.records("articles_view").imageUri(`${article.id}`, "image");
  }
  return "/default.svg";
}

const buttonStyle =
  "h-10 px-4 py-2 border border-input bg-pacamara-secondary hover:text-pacamara-accent foreground inline-flex items-center justify-center rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50";
const inputStyle =
  "flex h-10 w-full rounded-md border border-input px-3 py-2 text-sm ring-offset-background file:border-0 file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50";
const articleCardStyle =
  "group lg:mb-[50px] mb-10 last:mb-0 prose lg:prose-xl max-w-none prose-headings:font-bold prose-headings:text-pacamara-accent prose-p:text-pacamara-primary/70 lg:prose-p:text-[18px] prose-p:transition-all prose-p:duration-300 prose-a:font-semibold prose-a:text-pacamara-dark prose-a:hover:text-pacamara-pink prose-a:no-underline prose-a:transition-all prose-a:duration-300 prose-strong:font-normal prose-headings:font-pacamara-space prose-h2:mb-7 prose-h2:mt-0 prose-img:mt-0 prose-img:mb-0 dark:prose-a:text-white dark:prose-a:hover:text-pacamara-accent dark:prose-p:text-white/70";
const articleContentStyle =
  "lg:px-0 pt-10 mb-5 mx-auto prose lg:prose-xl prose-headings:transition-all prose-headings:duration-300 prose-headings:font-pacamara-space prose-headings:font-bold prose-headings:text-pacamara-accent prose-headings:mb-0 prose-headings:pb-3 prose-headings:mt-6 prose-p:transition-all prose-p:duration-300 prose-p:text-pacamara-primary/80 prose-li:transition-all prose-li:duration-300 prose-li:text-pacamara-primary/80 prose-td:transition-all prose-td:duration-300 prose-td:text-pacamara-primary/80 prose-a:underline prose-a:font-semibold prose-a:transition-all prose-a:duration-300 prose-a:text-pacamara-accent hover:prose-a:text-pacamara-dark prose-strong:transition-all prose-strong:duration-300 prose-strong:font-bold prose-hr:transition-all prose-hr:duration-300 prose-hr:border-pacamara-secondary/40 prose-img:rounded-lg prose-img:mx-auto prose-code:transition-all prose-code:duration-300 prose-code:text-pacamara-dark dark:prose-headings:text-pacamara-accent dark:prose-p:text-white/70 dark:prose-a:text-white dark:hover:prose-a:text-pacamara-accent dark:prose-strong:text-white dark:prose-li:text-white dark:prose-code:text-white dark:prose-td:text-white/70 dark:prose-hr:border-pacamara-accent/30 dark:prose-tr:border-pacamara-accent/30 dark:prose-thead:border-pacamara-accent/30";
