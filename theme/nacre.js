(() => {
  const KEYWORDS = new Set([
    "as",
    "async",
    "break",
    "const",
    "continue",
    "do",
    "else",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "match",
    "newtype",
    "pure",
    "return",
    "trait",
    "true",
    "type",
    "use",
    "while",
  ]);
  const TYPES = new Set([
    "Bool",
    "ExitCode",
    "Float",
    "Int",
    "Map",
    "None",
    "Option",
    "Path",
    "Result",
    "Some",
    "String",
    "Unit",
  ]);
  const BUILTINS = new Set([
    "args",
    "env",
    "fs",
    "hasCommand",
    "json",
    "pathExists",
    "process",
    "run",
  ]);

  function escapeHtml(value) {
    return value
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;");
  }

  function token(className, value) {
    return `<span class="nacre-${className}">${escapeHtml(value)}</span>`;
  }

  function highlightNacre(source) {
    let output = "";
    let index = 0;

    while (index < source.length) {
      const rest = source.slice(index);

      if (rest.startsWith("##")) {
        const end = source.indexOf("\n", index);
        const next = end === -1 ? source.length : end;
        output += token("comment", source.slice(index, next));
        index = next;
        continue;
      }

      const rawString = rest.startsWith('r"');
      const tripleString = rest.startsWith('"""');
      if (rawString || tripleString || source[index] === '"') {
        const delimiter = tripleString ? '"""' : '"';
        const start = index;
        index += rawString ? 2 : delimiter.length;
        while (index < source.length) {
          if (!rawString && source[index] === "\\") {
            index += 2;
            continue;
          }
          if (source.startsWith(delimiter, index)) {
            index += delimiter.length;
            break;
          }
          index += 1;
        }
        output += token("string", source.slice(start, index));
        continue;
      }

      const number = rest.match(/^(?:0x[\da-fA-F]+|0b[01]+|\d+(?:\.\d+)?)/);
      if (number) {
        output += token("number", number[0]);
        index += number[0].length;
        continue;
      }

      const identifier = rest.match(/^[A-Za-z_][A-Za-z0-9_]*/);
      if (identifier) {
        const value = identifier[0];
        const tail = source.slice(index + value.length);
        let className = "";
        if (KEYWORDS.has(value)) {
          className = "keyword";
        } else if (TYPES.has(value)) {
          className = "type";
        } else if (BUILTINS.has(value)) {
          className = "builtin";
        } else if (/^\s*\(/.test(tail)) {
          className = "function";
        }
        output += className ? token(className, value) : escapeHtml(value);
        index += value.length;
        continue;
      }

      const operator = rest.match(/^(?:<\||<\$>|<\*>|>>=|=>|->|<-|\+\+|&&|\|\||==|!=|<=|>=|<<|>>|[+\-*/%&|^~!<>=?:])/);
      if (operator) {
        output += token("operator", operator[0]);
        index += operator[0].length;
        continue;
      }

      output += escapeHtml(source[index]);
      index += 1;
    }

    return output;
  }

  function addLanguageSwitcher() {
    const main = document.querySelector(".content main");
    if (!main) {
      return;
    }

    const isJapanese = document.documentElement.lang === "ja";
    const filename = window.location.pathname.split("/").pop() || "index.html";
    const link = document.createElement("a");
    link.className = "nacre-language-link";
    link.href = isJapanese ? `../${filename}` : `ja/${filename}`;
    link.lang = isJapanese ? "en" : "ja";
    link.hreflang = link.lang;
    link.textContent = isJapanese ? "English" : "日本語";

    const alternate = document.createElement("link");
    alternate.rel = "alternate";
    alternate.hreflang = link.lang;
    alternate.href = link.href;
    document.head.append(alternate);

    const nav = document.createElement("nav");
    nav.className = "nacre-language-switcher";
    nav.setAttribute(
      "aria-label",
      isJapanese ? "表示言語" : "Documentation language",
    );
    nav.append(link);
    main.prepend(nav);
  }

  function searchIcon() {
    return `
      <span class="fa-svg" aria-hidden="true">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
          <path d="M416 208c0 46-15 88-40 123l127 126a32 32 0 0 1-46 46L331 376a208 208 0 1 1 85-168ZM208 352a144 144 0 1 0 0-288 144 144 0 0 0 0 288Z"/>
        </svg>
      </span>`;
  }

  function addSearch() {
    const controls = document.querySelector(".left-buttons");
    if (!controls) {
      return;
    }

    const isJapanese = document.documentElement.lang === "ja";
    const bundlePath = isJapanese ? "../pagefind/" : "pagefind/";
    const button = document.createElement("button");
    button.id = "nacre-search-toggle";
    button.className = "icon-button";
    button.type = "button";
    button.title = isJapanese ? "検索（/）" : "Search (/)";
    button.setAttribute("aria-label", button.title);
    button.setAttribute("aria-keyshortcuts", "/");
    button.innerHTML = searchIcon();
    controls.append(button);

    const dialog = document.createElement("dialog");
    dialog.className = "nacre-search-dialog";
    dialog.setAttribute(
      "aria-label",
      isJapanese ? "ドキュメント検索" : "Documentation search",
    );
    dialog.innerHTML = `
      <div class="nacre-search-header">
        <strong>${isJapanese ? "ドキュメント検索" : "Search documentation"}</strong>
        <button class="nacre-search-close" type="button" aria-label="${
          isJapanese ? "閉じる" : "Close"
        }">×</button>
      </div>
      <div id="nacre-pagefind"></div>`;
    document.body.append(dialog);

    let initialized = false;
    function initialize() {
      if (initialized) {
        return;
      }
      initialized = true;

      const stylesheet = document.createElement("link");
      stylesheet.rel = "stylesheet";
      stylesheet.href = `${bundlePath}pagefind-ui.css`;
      document.head.append(stylesheet);

      const script = document.createElement("script");
      script.src = `${bundlePath}pagefind-ui.js`;
      script.onload = () => {
        new window.PagefindUI({
          element: "#nacre-pagefind",
          bundlePath,
          showImages: false,
          showSubResults: true,
          autofocus: true,
          processResult(result) {
            if (result.url.startsWith("/")) {
              result.url = `${isJapanese ? ".." : "."}${result.url}`;
            }
            return result;
          },
        });
      };
      document.head.append(script);
    }

    function openSearch() {
      initialize();
      dialog.showModal();
    }

    button.addEventListener("click", openSearch);
    dialog
      .querySelector(".nacre-search-close")
      .addEventListener("click", () => dialog.close());
    dialog.addEventListener("click", (event) => {
      if (event.target === dialog) {
        dialog.close();
      }
    });
    document.addEventListener("keydown", (event) => {
      const target = event.target;
      const typing =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable;
      if (event.key === "/" && !typing) {
        event.preventDefault();
        openSearch();
      }
    });
  }

  function highlightCode() {
    document.querySelectorAll("code.language-nacre").forEach((element) => {
      element.innerHTML = highlightNacre(element.textContent);
      element.classList.add("nacre-highlighted");
    });
  }

  window.addEventListener("DOMContentLoaded", () => {
    addLanguageSwitcher();
    addSearch();
    highlightCode();
  });
})();
