import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, normalize, relative, resolve } from "node:path";

const root = resolve("site");
const errors = [];
const documentationPages = new Set([
  "index.html",
  "tutorial.html",
  "language-reference.html",
  "cli.html",
  "security-policy.html",
  "examples.html",
  "testing-and-coverage.html",
  "limitations.html",
  "syntax.html",
  "development-attribution.html",
]);

function walk(directory) {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

function localTarget(page, reference) {
  const withoutFragment = reference.split("#", 1)[0].split("?", 1)[0];
  if (!withoutFragment) {
    return reference.startsWith("#") ? page : null;
  }
  if (/^(?:[a-z]+:|\/\/)/i.test(withoutFragment)) {
    return null;
  }
  if (withoutFragment.startsWith("/nacre/")) {
    return join(root, withoutFragment.slice("/nacre/".length));
  }
  if (withoutFragment.startsWith("/")) {
    return null;
  }
  return normalize(resolve(dirname(page), decodeURIComponent(withoutFragment)));
}

function checkPage(path) {
  const html = readFileSync(path, "utf8");
  const displayPath = relative(root, path);
  const expectedLanguage = displayPath.startsWith(`ja${process.platform === "win32" ? "\\" : "/"}`)
    ? "ja"
    : "en";

  if (!html.includes(`<html lang="${expectedLanguage}"`)) {
    errors.push(`${displayPath}: expected lang="${expectedLanguage}"`);
  }
  if (
    documentationPages.has(displayPath.split(/[/\\]/).pop()) &&
    !html.includes("<main data-pagefind-body>")
  ) {
    errors.push(`${displayPath}: main content is not marked for Pagefind`);
  }

  for (const match of html.matchAll(/\b(?:href|src)="([^"]+)"/g)) {
    const target = localTarget(path, match[1]);
    if (!target) {
      continue;
    }
    const relativeTarget = relative(root, target);
    if (relativeTarget.startsWith("..")) {
      errors.push(`${displayPath}: reference escapes the published site ${match[1]}`);
    } else if (!existsSync(target)) {
      errors.push(`${displayPath}: broken local reference ${match[1]}`);
    } else if (match[1].includes("#") && target.endsWith(".html")) {
      const fragment = decodeURIComponent(match[1].split("#", 2)[1]);
      if (
        fragment &&
        !readFileSync(target, "utf8").includes(`id="${fragment}"`)
      ) {
        errors.push(`${displayPath}: missing fragment target ${match[1]}`);
      }
    }
  }
}

for (const required of [
  "index.html",
  "ja/index.html",
  "assets/nacre-compiler.png",
  "examples/control-flow.ncr",
  "examples/japanese.ncr",
  "pagefind/pagefind-entry.json",
  "pagefind/pagefind-ui.css",
  "pagefind/pagefind-ui.js",
]) {
  if (!existsSync(join(root, required))) {
    errors.push(`missing generated file: ${required}`);
  }
}

const htmlPages = walk(root).filter((path) => path.endsWith(".html"));
htmlPages.forEach(checkPage);

const pagefind = JSON.parse(
  readFileSync(join(root, "pagefind/pagefind-entry.json"), "utf8"),
);
for (const language of ["en", "ja"]) {
  if (pagefind.languages?.[language]?.page_count !== 10) {
    errors.push(`Pagefind did not index all 10 ${language} documentation pages`);
  }
}

for (const page of ["index.html", "ja/index.html"]) {
  const html = readFileSync(join(root, page), "utf8");
  if (html.includes("elasticlunr") || html.includes("mdbook-search-toggle")) {
    errors.push(`${page}: legacy mdBook search is still enabled`);
  }
}

for (const page of ["examples.html", "ja/examples.html"]) {
  const path = join(root, page);
  if (!existsSync(path)) {
    continue;
  }
  const html = readFileSync(path, "utf8");
  if (!html.includes('    run.output.echo("name: ${name}")')) {
    errors.push(`${page}: nested Nacre code lost its four-space indentation`);
  }
  if (!html.includes("こんにちは、${name}さん")) {
    errors.push(`${page}: verified Japanese source example is missing`);
  }
}

const englishPages = readdirSync(root)
  .filter((name) => name.endsWith(".html") && !["404.html", "print.html"].includes(name))
  .sort();
const japanesePages = readdirSync(join(root, "ja"))
  .filter((name) => name.endsWith(".html") && !["404.html", "print.html"].includes(name))
  .sort();

if (JSON.stringify(englishPages) !== JSON.stringify(japanesePages)) {
  errors.push("English and Japanese page sets do not match");
}

if (errors.length > 0) {
  console.error(errors.map((error) => `- ${error}`).join("\n"));
  process.exit(1);
}

console.log(
  `Verified ${htmlPages.length} HTML pages, bilingual parity, local links, assets, and code indentation.`,
);
