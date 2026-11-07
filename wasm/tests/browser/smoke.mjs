// Chromium smoke test: serve the built module over HTTP and call it in a
// real page. `wasm/build.sh && cd wasm && bun install && bun run test:browser`
// WORKOUT_WASM_PKG names another build directory (default wasm/pkg).
import { chromium } from "playwright"
import { resolve } from "node:path"

const pkg = resolve(process.env.WORKOUT_WASM_PKG ?? resolve(import.meta.dir, "../../pkg"))
const page = `<!doctype html><meta charset="utf-8"><title>wasm smoke</title>
<script type="module">
  import init, * as lang from "/wasm/workout_wasm.js"
  await init()
  const line = "- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰"
  const names = lang.kinds()
  const flat = lang.tokenize(line)
  const tokens = []
  for (let i = 0; i < flat.length; i += 3) tokens.push([line.slice(flat[i], flat[i + 1]), names[flat[i + 2]]])
  window.result = {
    tokens,
    array: flat instanceof Uint32Array,
    diagnostics: JSON.parse(lang.diagnostics("2026-11-07\\n- Pec Fly 39kg\\n")),
    unfinished: lang.unfinished_date("2026-11-0\\n"),
    version: lang.version(),
  }
</script>`

const server = Bun.serve({
  port: 0,
  async fetch(req) {
    const path = new URL(req.url).pathname
    if (path === "/") return new Response(page, { headers: { "content-type": "text/html; charset=utf-8" } })
    const name = path.match(/^\/wasm\/(workout_wasm(?:_bg\.wasm|\.js))$/)?.[1]
    if (!name) return new Response("not found", { status: 404 })
    const type = name.endsWith(".wasm") ? "application/wasm" : "text/javascript"
    return new Response(Bun.file(resolve(pkg, name)), { headers: { "content-type": type } })
  },
})

const fail = msg => {
  console.error(`FAIL ${msg}`)
  process.exitCode = 1
}
const browser = await chromium.launch()
try {
  const tab = await browser.newPage()
  const errors = []
  tab.on("pageerror", e => errors.push(String(e)))
  await tab.goto(`http://127.0.0.1:${server.port}/`)
  const r = await tab.waitForFunction(() => window.result, null, { timeout: 10000 }).then(h => h.jsonValue())
  if (errors.length) fail(`page errors: ${errors.join("; ")}`)
  if (!r.array) fail("tokenize did not return a Uint32Array")
  const want = [["-", "bullet"], ["Treadmill", "exercise"], ["1.57", "distance"], ["km", "unit"]]
  if (JSON.stringify(r.tokens.slice(0, 4)) !== JSON.stringify(want)) fail(`tokens ${JSON.stringify(r.tokens)}`)
  if (JSON.stringify(r.tokens.at(-1)) !== JSON.stringify(["⁰", "unit"])) fail(`last token ${JSON.stringify(r.tokens.at(-1))}`)
  if (r.diagnostics[0]?.message !== "39kg has no reps: write 12x39kg or 39kg 12") fail(`diagnostics ${JSON.stringify(r.diagnostics)}`)
  if (r.unfinished !== true) fail("unfinished_date")
  if (!process.exitCode) console.log(`ok: Chromium ${browser.version()}, ${r.tokens.length} tokens, version ${r.version}`)
} finally {
  await browser.close()
  server.stop(true)
}
