// The built module in Node: `wasm/build.sh && node --test wasm/tests/js/`.
// WORKOUT_WASM_PKG names another build directory (default wasm/pkg).
import { test } from "node:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { fileURLToPath, pathToFileURL } from "node:url"
import { resolve } from "node:path"

const here = fileURLToPath(new URL(".", import.meta.url))
const pkg = resolve(process.env.WORKOUT_WASM_PKG ?? resolve(here, "../../pkg"))
const lang = await import(pathToFileURL(resolve(pkg, "workout_wasm.js")).href)
lang.initSync({ module: readFileSync(resolve(pkg, "workout_wasm_bg.wasm")) })

const LOG = readFileSync(resolve(here, "../../../tests/fixtures/Workout.md"), "utf8")
const LINES = LOG.split("\n")
const fixtureLine = s => {
  const n = LINES.indexOf(s)
  assert.ok(n >= 0, `fixture has ${s}`)
  return n
}

/** [[text, kind], ...] of a line, sliced with the returned columns */
function tokens(line) {
  const names = lang.kinds()
  const flat = lang.tokenize(line)
  assert.ok(flat instanceof Uint32Array)
  assert.equal(flat.length % 3, 0)
  const out = []
  for (let i = 0; i < flat.length; i += 3) out.push([line.slice(flat[i], flat[i + 1]), names[flat[i + 2]]])
  return out
}

test("kinds", () => {
  const k = lang.kinds()
  assert.equal(k.length, 24)
  assert.equal(k[0], "date")
  assert.ok(k.includes("exercise") && k.includes("invalid"))
})

test("tokenize a fixture line", () => {
  const line = "- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰"
  fixtureLine(line)
  assert.deepEqual(tokens(line), [
    ["-", "bullet"], ["Treadmill", "exercise"],
    ["1.57", "distance"], ["km", "unit"],
    ["20", "duration"], ["min", "unit"], ["@", "operator"],
    ["4.7", "speed"], ["km/h", "unit"],
    ["4.5", "range"], ["~", "operator"], ["15", "range"], ["⁰", "unit"],
  ])
  const flat = lang.tokenize(line)
  assert.equal(flat[flat.length - 2], line.length, "UTF-16 columns end at the JS length")
})

test("tokenize counts UTF-16 units", () => {
  const t = tokens("- Bench 💪 12x40kg")
  assert.deepEqual(t[2], ["💪", "invalid"])
  assert.deepEqual(t[3], ["12", "reps"])
  assert.equal(tokens("- Run 20min 4.5~15º").at(-1)[0], "º")
  assert.equal(lang.tokenize("").length, 0)
})

test("diagnostics", () => {
  const d = JSON.parse(lang.diagnostics("2026-11-07\n- Pec Fly 39kg\n"))
  assert.deepEqual(d, [{
    line: 1, from: 10, to: 14, severity: "error",
    message: "39kg has no reps: write 12x39kg or 39kg 12", fix: null,
  }])
  const [w] = JSON.parse(lang.diagnostics("2026-11-07 82500g\n"))
  assert.equal(w.line, 0)
  assert.deepEqual(w.fix, { title: "Write `82.5kg`", from: 11, to: 17, text: "82.5kg" })
  const line = "2026-11-07 82500g"
  assert.equal(line.slice(0, w.fix.from) + w.fix.text + line.slice(w.fix.to), "2026-11-07 82.5kg")
  // after an emoji the columns are JS string indexes
  const [e] = JSON.parse(lang.diagnostics("2026-11-07\n- 💪 Pec Fly\n"))
  assert.equal("- 💪 Pec Fly".slice(e.from, e.to), "💪")
})

test("a session after a non-empty line gets a fix inserting an empty line", () => {
  const text = "2026-11-06\n- Row 12x20kg\n2026-11-07\n- Row 12x25kg\n"
  const d = JSON.parse(lang.diagnostics(text))
  assert.deepEqual(d, [{
    line: 2, from: 0, to: 10, severity: "warning",
    message: "a session starts after an empty line: add one before 2026-11-07",
    fix: { title: "Add an empty line before 2026-11-07", from: 0, to: 0, text: "\n" },
  }])
  const lines = text.split("\n")
  const l = lines[d[0].line]
  lines[d[0].line] = l.slice(0, d[0].fix.from) + d[0].fix.text + l.slice(d[0].fix.to)
  const fixed = lines.join("\n")
  assert.equal(fixed, "2026-11-06\n- Row 12x20kg\n\n2026-11-07\n- Row 12x25kg\n")
  assert.deepEqual(JSON.parse(lang.diagnostics(fixed)), [])
})

test("diagnostics of the whole fixture", () => {
  const d = JSON.parse(lang.diagnostics(LOG))
  assert.ok(d.length > 10)
  for (const x of d) {
    assert.ok(x.severity === "error" || x.severity === "warning")
    assert.ok(x.from <= x.to && x.to <= LINES[x.line].length, JSON.stringify(x))
  }
  const bw = d.find(x => x.fix?.text === "bw")
  assert.equal(LINES[bw.line].slice(bw.fix.from, bw.fix.to), "BW")
})

test("diagnostics of the whole fixture take under 50 ms", () => {
  lang.diagnostics(LOG) // warm up
  const runs = 20
  const t0 = performance.now()
  for (let i = 0; i < runs; i++) lang.diagnostics(LOG)
  const ms = (performance.now() - t0) / runs
  console.log(`# diagnostics of ${LINES.length} lines: ${ms.toFixed(2)} ms`)
  assert.ok(ms < 50, `${ms} ms`)
})

test("completions", () => {
  const text = LOG + "\n2026-11-08\n- Pec"
  const line = text.split("\n").length - 1
  const c = JSON.parse(lang.completions(text, line, 5))
  assert.deepEqual(c[0], { label: "Pec Fly", detail: "39kg 12^3", from: 2, to: 5, insert: "Pec Fly 39kg 12^3" })
  for (const x of c) assert.deepEqual(Object.keys(x).sort(), ["detail", "from", "insert", "label", "to"])
  const [w] = JSON.parse(lang.completions("2026-11-06 82.5kg\n2026-11-07", 1, 10))
  assert.deepEqual(w, { label: "82.5kg", detail: "body weight on 2026-11-06", from: 10, to: 10, insert: " 82.5kg" })
})

test("hover", () => {
  const n = fixtureLine("- Pec Fly 39kg 12^3")
  assert.match(lang.hover(LOG, n, 3), /\*\*Pec Fly\*\*/)
  assert.equal(lang.hover(LOG, LINES.indexOf(""), 0), undefined)
})

test("unfinished_date", () => {
  assert.equal(lang.unfinished_date("2026-11-0\n"), true)
  assert.equal(lang.unfinished_date("07/11/2026\n- Row 12x40kg\n"), true)
  assert.equal(lang.unfinished_date(LOG), false)
})

test("version", () => {
  assert.match(lang.version(), /^\d+\.\d+\.\d+/)
})

test("every function is total", () => {
  for (const [l, c] of [[-1, -1], [1e9, 0], [0, 1e9], [NaN, NaN], [2 ** 32 + 3, 0.5]]) {
    assert.ok(Array.isArray(JSON.parse(lang.completions(LOG, l, c))))
    lang.hover(LOG, l, c)
    assert.ok(Array.isArray(JSON.parse(lang.completions("", l, c))))
    assert.equal(lang.hover("", l, c), undefined)
  }
  for (const s of ["", "\n", "\r\n", "💪", "\u0000", "\ud800", "[[2026-", "_", "~", ";", "- ".repeat(5000)]) {
    lang.tokenize(s)
    JSON.parse(lang.diagnostics(s))
    lang.unfinished_date(s)
  }
  // every prefix of the last sessions, as typed
  const tail = LINES.slice(-40).join("\n")
  for (let i = 0; i <= tail.length; i++) {
    const t = tail.slice(0, i)
    const rows = t.split("\n")
    const row = rows.length - 1
    JSON.parse(lang.diagnostics(t))
    JSON.parse(lang.completions(t, row, rows[row].length))
    lang.hover(t, row, rows[row].length)
    lang.tokenize(rows[row])
  }
})
