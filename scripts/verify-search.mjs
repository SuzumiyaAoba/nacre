import { createReadStream, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join } from "node:path";

const root = process.cwd();
const server = createServer((request, response) => {
  const path = join(root, new URL(request.url, "http://localhost").pathname);
  try {
    statSync(path);
    response.setHeader(
      "Content-Type",
      [".pagefind", ".wasm"].includes(extname(path))
        ? "application/wasm"
        : "application/octet-stream",
    );
    createReadStream(path).pipe(response);
  } catch {
    response.statusCode = 404;
    response.end();
  }
});

await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const { port } = server.address();

globalThis.document = {
  currentScript: null,
  querySelector() {
    return { getAttribute: () => globalThis.testLanguage };
  },
};

const pagefind = await import("../site/pagefind/pagefind.js");
const cases = [
  { language: "ja", query: "実行ポリシー", expectedPath: "/ja/" },
  { language: "en", query: "execution policy", expectedPath: "/" },
];

try {
  for (const { language, query, expectedPath } of cases) {
    globalThis.testLanguage = language;
    await pagefind.options({
      basePath: `http://127.0.0.1:${port}/site/pagefind/`,
      noWorker: true,
    });
    const search = await pagefind.search(query);
    if (search.results.length === 0) {
      throw new Error(`${language} search returned no results for ${query}`);
    }

    const result = await search.results[0].data();
    const resultPath = new URL(result.url, "http://localhost").pathname;
    if (
      expectedPath === "/ja/" ? !resultPath.includes("/ja/") : resultPath.includes("/ja/")
    ) {
      throw new Error(
        `${language} search returned a result from the wrong language: ${result.url}`,
      );
    }

    console.log(
      `${language} search: ${search.results.length} results for "${query}"`,
    );
    await pagefind.destroy();
  }
} finally {
  server.close();
}
