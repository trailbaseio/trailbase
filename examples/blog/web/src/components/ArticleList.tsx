import { createResource, createSignal, For, Match, Switch } from "solid-js";
import { useStore } from "@nanostores/solid";
import type { Client } from "trailbase";

import { $client } from "@/lib/client";
import { $profile, createProfile } from "@/lib/profile";
import { imageUrl, inputStyle, buttonStyle } from "@/lib/article";

import { Tag } from "@/components/Tag";
import { PublishDate } from "@/components/PublishDate";

import type { Article } from "@schema/article";

function ArticleCard(props: { article: Article; index: number }) {
  const isOdd = () => props.index % 2;

  const link = () => `/article/?id=${props.article.id}`;

  return (
    <article class={articleCardStyle}>
      <div class="grid grid-cols-1 items-center gap-10 md:grid-cols-[200px_auto] lg:grid-cols-[200px_auto]">
        <div>
          <a href={link()}>
            <img
              src={imageUrl(props.article)}
              width="750"
              alt={props.article.title + "Thumbnail"}
              class={`image-shine h-[200px] rounded-[15px] object-cover ${isOdd() ? "rotate-2" : "-rotate-2"}`}
            />
          </a>
        </div>

        <div>
          <h2>
            <a
              href={link()}
              class="text-pacamara-dark hover:text-pacamara-accent"
            >
              {props.article.title}
            </a>
          </h2>

          <p>{props.article.intro}</p>

          <p class="mt-5 flex flex-row flex-wrap items-center gap-4 group-last:mb-0">
            <Tag tag={props.article.tag} />
            <PublishDate date={props.article.created} />
            <span>by {props.article.username}</span>
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
  const [articles] = createResource(client, async (client) => {
    const response = await client?.records<Article>("articles_view").list();
    return response.records;
  });
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

const articleCardStyle =
  "group lg:mb-[50px] mb-10 last:mb-0 prose lg:prose-xl max-w-none prose-headings:font-bold prose-headings:text-pacamara-accent prose-p:text-pacamara-primary/70 lg:prose-p:text-[18px] prose-p:transition-all prose-p:duration-300 prose-a:font-semibold prose-a:text-pacamara-dark prose-a:hover:text-pacamara-pink prose-a:no-underline prose-a:transition-all prose-a:duration-300 prose-strong:font-normal prose-headings:font-pacamara-space prose-h2:mb-7 prose-h2:mt-0 prose-img:mt-0 prose-img:mb-0 dark:prose-a:text-white dark:prose-a:hover:text-pacamara-accent dark:prose-p:text-white/70";
