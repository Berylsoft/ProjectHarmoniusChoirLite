class Selector extends HTMLElement {
  constructor() {
    super();

    this.addEventListener("click", (e) => {
      if (this.hasAttribute("readonly")) return;

      let t = e.target;
      if (t === this) return;
      while (t.parentNode !== this) {
        t = t.parentNode;
        if (!t) return;
      }

      const d = t.dataset;

      if (!this.hasAttribute("multi"))
        for (const it of this.children) delete it.dataset.selected;

      if ("selected" in d) delete d.selected;
      else d.selected = "";

      if (this.onchange) {
        const idx = Array.prototype.indexOf.call(this.children, t);
        const next = this.onchange(
          Array.prototype.map.call(
            this.children,
            (it) => "selected" in it.dataset,
          ),
          idx,
        );

        if (next instanceof Array) {
          if (next.length !== this.children.length)
            throw new Error("next's length should match selector items");

          for (const i in next) {
            const d = this.children[i].dataset;

            if (next[i]) d.selected = "";
            else if ("selected" in d) delete d.selected;
          }
        }
      }
    });
  }
}

globalThis.customElements.define("comp-selector", Selector);
