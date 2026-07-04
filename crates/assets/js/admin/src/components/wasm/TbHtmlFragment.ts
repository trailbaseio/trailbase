// Script tags inserted via innerHTML never execute, so each one is cloned and
// replaced to make the browser treat it as newly inserted.
export class TbHtmlFragment extends HTMLElement {
  #root = this.attachShadow({ mode: "open" });

  get html(): string {
    return this.#root.innerHTML;
  }

  set html(value: string) {
    this.#root.innerHTML = value;
    this.#root.querySelectorAll("script").forEach((old) => {
      const next = document.createElement("script");
      Array.from(old.attributes).forEach((attr) => {
        next.setAttribute(attr.name, attr.value);
      });
      next.textContent = old.textContent;
      old.parentNode?.replaceChild(next, old);
    });
  }
}

customElements.define("tb-html-fragment", TbHtmlFragment);

declare module "solid-js" {
  namespace JSX {
    interface IntrinsicElements {
      "tb-html-fragment": HTMLAttributes<TbHtmlFragment>;
    }
  }
}
