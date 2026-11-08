# Workout Syntax — Grammar

Formal grammar for the plain-text workout log. The executable version is
the PEG in `src/workout.pest`; this file is the same grammar in EBNF plus
the semantic rules the parser applies. The log is read **one line at a
time**; document structure is built from the sequence of lines.

## 1. Document

```ebnf
document  = { line , newline } ;
line      = date-line | time-line | superset-label | exercise-line | blank-line ;

date-line      = ( link-open , date , link-close | date ) , [ sp , body-weight ] ;
                                                           (* [[2026-05-12]]  2026-09-21  2026-11-07 82.5kg *)
date           = digit{4} , "-" , digit{2} , "-" , digit{2} ;
body-weight    = number , ( "kg" | "g" | "lbs" | "lb" | "oz" ) ;  (* lowercase; uppercase warns *)
time-line      = clock , range-op , [ clock ] | range-op , clock | clock ;   (* 18h15~19h15  ~17h00  12h55~  18h15 *)
clock          = clock-hour , hour-mark , clock-minute ;   (* 18h15 *)
clock-hour     = digit , [ digit ] ;
clock-minute   = digit{2} ;
superset-label = label-mark , label-text , label-mark ;    (* _Lat Pulldown Super Set_ *)
blank-line     = [ comment ] ;
```

**Named symbols.** Every piece of syntax is its own rule, so the parse tree
gives each symbol a span (for precise errors, and so highlighting could
come from the parser). These are all the symbols:

```ebnf
link-open  = "[[" ;          link-close = "]]" ;
range-op   = "~" ;           (* time ranges and cardio ranges *)
hour-mark  = "h" ;           (* 18h15 *)
label-mark = "_" ;           (* around a super-set label *)
bullet     = "-" | "+" ;
at         = "@" ;           (* machine, speed *)
plus       = "+" ;           (* +22kg, bw+10kg *)
minus      = "-" ;           (* bw-36kg *)
step-op    = "++" ;          (* 63kg++5lbs *)
by         = "x" ;           (* the x of a set: 12x63kg *)
side-sep   = "/" ;           (* 12/11 *)
repeat-op  = "^" | "*" ;     (* 12^3, 12*3 *)
colon      = ":" ;           (* 11:26 *)
```

Units (`mass-unit`, `dist-unit`, `min-unit`, `sec-unit`, `speed-unit`,
`angle-unit`), `bw-word` and `sides` are named rules too. On every line
the parser accepts, each non-whitespace character outside a comment lies
in a named leaf (a rule with no sub-rules); a test checks this on both
logs. A name and a machine name are one leaf each.

Every line may be indented and may end with `; comment` (except inside a
super-set label, which ends at the closing `_`).

**Body weight** is written on the date line, after the date and a space:
`2026-11-07 82.5kg` or `[[2026-11-07]] 182lbs`, then optionally `; comment`.
The number uses a decimal point (`82,5kg` is an error) and the unit is
attached (`82.5 kg` is an error). It belongs to the session
(`Session::body_weight`), which keeps the number and unit as written and
converts to kg (1 lb = 0.45359237 kg, 1 oz = 0.028349523125 kg,
1 g = 0.001 kg). Each date is still written once: a weigh-in day with no
workout is a session with no exercises, and that is not a warning. A
weigh-in on a date that does not exist does not count, since the line starts
no session.

**Dates and times must exist.** A date's month is 01–12 and its day is
valid for that month, counting leap years (`2028-02-29` is valid,
`2026-02-29` is not); a clock's hours are 0–23 and its minutes 00–59. The
grammar only checks the shape, so `2026-02-30` and `25h70` parse, and the
document check reports them as errors. A date that does not exist starts no
session: it closes the open super set like any date, and the lines after it,
up to the next real date, belong to no session (they are still checked). A
time that does not exist is not recorded.

**Half-typed dates** (`2026-`, `2026-11`, `2026-11-0`, `2026-11-7`,
`[[2026-11-07` without `]]`) are a warning, *date is incomplete*, instead of a
parse error; like an impossible date, they start no session. Every such
message starts with `date is incomplete` (`dates::INCOMPLETE_DATE`), and
`Diagnostic::is_incomplete_date` / `Document::has_incomplete_date` find them.

**Other date formats** are a warning only, with no quick fix: the owner
corrects every date by hand. Year first `2026/11/07`, `2026.11.07`, or day
first `07/11/2026`, `07-11-2026`, `07.11.2026` (read as day/month/year). The
message suggests the likely date (*did you mean 2026-11-07? please correct
the date*). Until corrected, the line starts no session. If the date does not
exist (`30/02/2026`) the message says so instead.

**A one-digit month with `-`** (`2026-1-07`, `2026-1-7`) is treated as a
typo, since `2026-1-07` could mean 2026-01-07, 2026-10-07, 2026-11-07 or
2026-12-07. It is a warning with no fix: the message lists the possible
dates when there are at most four and asks the owner to correct it. It
starts no session.

While any date line is half-typed, in another format or impossible, the
app holds syncing (`Document::has_unfinished_date`): such a line starts no
session, so the sets under it have no date of their own.

**Session time.** The time line after the date is `18h15~19h40`, an open
range (`~17h00`, `12h55~`) or a start time alone (`18h15`). Start and end
are kept as minutes after midnight, and the duration is shown in hover and
the outline (`18h15~19h40 · 1 h 25 min`). No session lasts more than 6 hours:

| Line | Meaning |
|---|---|
| `23h30~00h40` | crosses midnight: 1 h 10 min |
| `19h40~18h15` | warning, typo: *19h40~18h15 ends before it starts: did you mean 18h15~19h40? please correct the times*, with a quick fix that swaps the times |
| `08h00~15h30` | warning: *lasts 7 h 30 min, but a session lasts at most 6 h* |
| `18h15~18h15` | warning: *the session starts and ends at 18h15* |

An end earlier than the start crosses midnight when that gives at most 6
hours; otherwise it is the typo above (no fix when the swap is also over 6 h).

**Which lines start a session** is decided by one function,
`session::session_date`, used by both the parser and the merge, so the two
always cut the log into the same sessions (a test checks about 96,000 lines).
A line starts a session when, after optional spaces or tabs, it has a real
date `yyyy-mm-dd` or `[[yyyy-mm-dd]]`, then optionally a body weight (a
number with a point or comma and a unit `kg g lb lbs oz` in any case), then
optionally a `; comment`, and nothing else. A body weight that is wrong is an
error on the weight alone; the session still starts, so its sets keep their
date:

| Line | Error | Quick fix |
|---|---|---|
| `2026-11-07 82500g` | `a body weight of 82500g is over the limit of 9999g: write 82.5kg` | `82.5kg` |
| `2026-11-07 82,5kg` | `body weights use a decimal point: write 82.5kg` | `82.5kg` |
| `2026-11-07 9999.5kg` | `a body weight of 9999.5kg is over the limit of 9999kg` | none |

## 2. Exercise line

```ebnf
exercise-line = bullet , sp , name , [ sp , machine ] , [ sp , initial ] ,
                [ sp , sides ] , [ sp , body ] , [ comment ] ;
bullet        = "-" | "+" ;                 (* "+" marks a super-set member *)
name          = name-word , { sp , name-word } ;
name-word     = letter , { non-space }      (* Pulldown, SL7026 *)
              | number , angle , { non-space } ;            (* 45º *)
machine       = at , alnum , { non-space } , { sp , name-word } ;  (* @Step Up Machine, @322RXS *)
initial       = plus , mass ;                (* +22kg: bar / sled, added once *)
sides         = "x2" | "2x" ;               (* written load is per side *)
comment       = ";" , { any } ;
```

A name ends at the first data token. Unit words (`kg`, `lb`, `lbs`, `km`,
`mi`), `x2`/`2x` and `bw…` can never be name words, so `Treadmill km 16min`
is an error instead of an exercise called "Treadmill km".

**Letter case.** Units (`kg`, `lb`, `lbs`, `km`, `mi`, `km/h`, `kph`,
`mph`, `min`, `m`, `sec`, `s`, and `g`, `oz` of a body weight), `bw`, `x2`/`2x` and the `x` of a set must be
lowercase. Uppercase still parses, with a warning and a quick fix, so
`Biceps Curl X2 12x8kg` keeps its doubling (12×16kg) instead of putting
`X2` in the name, and `12X20kg` is still a set.

## 3. Loads

```ebnf
mass         = number , mass-unit ;
mass-unit    = "kg" | "lbs" | "lb" ;                  (* lowercase; uppercase warns *)
bodyweight   = bw-word , [ ( plus | minus ) , mass ] ;   (* bw, bw+10kg, bw-36kg *)
bw-word      = "bw" ;                                 (* lowercase; uppercase warns *)
incremental  = mass , step-op , mass ;                (* 63kg++5lbs *)
default-load = incremental | bodyweight | mass ;
number       = digit , { digit } , [ "." , digit , { digit } ] ;
```

## 4. Strength body

```ebnf
body         = cardio | strength ;
strength     = default-load , [ sp , sides ] , sp , set-list
             | set-list ;
set-list     = set-item , { sp , set-item } ;
set-item     = explicit-set | explicit-hold | hold-set | bare-set ;
explicit-set = ( reps , by , ( bodyweight | mass ) | mass , by , reps ) , [ repeat ] ;
                                              (* 12x63kg  39kgx12  12x63kg^2 *)
bare-set     = reps , [ repeat ] ;            (* 12  12^3  12/11 *)
explicit-hold = ( hold , by , ( bodyweight | mass ) | mass , by , hold ) , [ repeat ] ;
                                              (* 60sx20kg  20kgx1m30s^2 *)
hold-set     = hold , [ repeat ] ;            (* 60s  60s^2 *)
hold         = count , colon , count          (* 1:30 *)
             | count , min-unit , [ count , sec-unit ]   (* 1min 1m30s *)
             | count , sec-unit ;             (* 60s 45sec *)
reps         = count , side-sep , count       (* left / right *)
             | count ;
repeat       = repeat-op , count ;
```

**A load needs reps.** A load standing alone (`- Pec Fly 39kg`, also with
`x2`, an initial weight or a machine) is still an error, but the message
says what is missing, with the span on the load:

| Line | Message |
|---|---|
| `- Pec Fly 39kg` | `39kg has no reps: write 12x39kg or 39kg 12` |
| `- Pull Up bw` | `bw has no reps: write 12xbw or bw 12` |
| `- Chin Assisted bw-36kg` | `bw-36kg has no reps: write 12xbw-36kg or bw-36kg 12` |
| `- Press 63kg++5lbs` | `63kg++5lbs has no reps: write 63kg++5lbs 12` |

An initial weight alone (`- Bench Press +22kg`) is not a load lifted but
the bar, so the line parses and the document warns that it has nothing
recorded, as for a bare name.

### Timed sets

A set can be a hold time instead of reps: `- Plank bw 60s^2` means the
plank was held at body weight for 60 seconds, twice. A hold uses the cardio
duration forms with no speed: `60s`, `45sec`, `1min`, `1m30s`, `1:30`, units
in lowercase (uppercase parses with the usual warning and quick fix).

| Line | Sets |
|---|---|
| `- Plank bw 60s^2` | 2 × 1 min at bw |
| `- Plank bw 60s 45s 30s` | 1 min, 45 s, 30 s at bw |
| `- Farmer Walk 30kg 40s^3` | 3 × 40 s at 30 kg |
| `- Farmer Walk x2 24kg 40s` | 40 s at 48 kg (per side, as for reps) |
| `- Dead Hang 60sx20kg 20kgx45s` | explicit loads, like `12x20kg` / `20kgx12` |
| `- Farmer Walk 20kg++5kg 30s^3` | 30 s at 20, 25, 30 kg |
| `- Plank 60s^2` | 2 × 1 min; no load written means body weight |

`bw` is the norm in the log; a bare hold with no load at all is held at
body weight (it is not an R1 error, unlike bare reps). Rule R2 applies to
holds too: `- Hang 60sx20kg 45s` is an error (*set 2 has a time only, but an
earlier set gave an explicit weight…*).

**Cardio or timed.** The body is read as cardio first, and stays cardio
when the whole body is cardio metrics (distances, durations with or without
a speed, ranges). Otherwise it is a strength body, where durations are
holds. So holds are timed sets when the line has a load or `bw`
(`bw 60s`, `30kg 40s`), a repeat on a hold (`60s^2`, `60s 45s*2`), or an
explicit load (`60sx20kg`). Durations alone stay cardio exactly as before:
`- Bike 20min`, `- Treadmill 11:26`, `- Treadmill 5min 20min` and also
`- Plank 60s 45s 30s` (with the several-durations error) or `- Plank 60s`
(with the no-distance warning). When such a cardio line has only durations
in seconds (`60s`, `45sec`), both messages suggest the timed form:
*… for timed sets, write the load first, such as `bw 60s 45s 30s`; for
cardio, keep one and correct the line* and *no distance; for a timed set,
write the load first, such as `bw 60s`*. Every line of the fixture log
parses to the same body as before (`tests/body_types.rs`).

**One kind per line.** A line has reps or timed sets, not both:
`- Plank bw 12 60s` is an error on `60s`: *`60s` is a timed set, but the
sets before it are reps: a line has reps or timed sets, not both, so write
them on two lines* (and *`12` is reps, but the sets before it are timed…*
the other way round).

**A hold lasts more than 0**: `- Plank bw 0s` is an error, *a timed set of
0s holds for no time: a hold lasts at least 1s*.

**Totals.** A timed set counts as a set and adds to the time held, not to
reps or volume. Hover shows a `Hold` column (`| 1 | 1 min | bw | bw |`) and
`2 sets · 2 min held`; a session totals `… · 24 reps · 4 min 10 s held · 480
kg volume`; the inlay hint is `2 min held`. The removal check compares holds
like reps: a lost timed set is *removes 1 set of Plank on 2026-11-07 (2 to
1)*, a shorter one *reduces set 2 of Plank on 2026-11-07 from 1 min to 45 s*.

## 5. Cardio body

```ebnf
cardio   = cardio-metric , { sp , cardio-metric } ;
cardio-metric = distance | duration | range ;
distance = number , dist-unit ;                       (* 1.57km *)
duration = ( count , colon , count                    (* 11:26 *)
           | count , min-unit , [ count , sec-unit ]  (* 20min 14m28s *)
           | count , sec-unit ) ,
           [ at , speed ] ;                           (* 20min@4.7km/h *)
speed    = number , speed-unit ;
range    = number , range-op , number , [ angle-unit ] ;            (* 4.5~15⁰ *)
dist-unit  = "km" | "mi" ;
min-unit   = "min" | "m" ;
sec-unit   = "sec" | "s" ;
speed-unit = "km/h" | "kph" | "mph" ;
angle-unit = "⁰" | "°" | "º" | "%" ;
```

In cardio, `m` means **minutes**; distances are always `km` or `mi`. All
units are lowercase; uppercase parses with a warning.

**Segments need speeds.** Several durations on one cardio line are
segments only when every one has a speed (`10min@6km/h 10min@3km/h`): each
segment's distance is time × speed, and they add up. When one has no speed
(`5min 20min`, `5min 20min@4.7km/h`), the line contradicts itself: an error
with no fix, asking the owner to correct it, usually by turning the first
into a distance: *a cardio line has one duration unless each has a speed,
but this one has 2 (`5min`, `20min`): turn the first into a distance, such
as `5km 20min`, and correct the line*.

**Totals.** Hover and the outline show a cardio line's total time,
distance and pace: `total 20 min · 1.5 km (from speed) · 13:20 /km`. The
distance is the one written, or else the sum of time × speed.

**Distance is expected.** A cardio line with a duration but no distance
(on any machine: treadmill, bike, elliptical) warns `no distance`. When
every duration has a speed, the warning shows time × speed (summed over
segments) and offers a quick fix that inserts it, in km rounded to 2
decimals, before the first duration; `mph` is converted first:

```
- Treadmill 20min@4.7km/h 4.5~15⁰         no distance: 20 min at 4.7 km/h is 1.57 km
- Treadmill 1.57km 20min@4.7km/h 4.5~15⁰  (after the fix)
- Treadmill 11:26                         no distance (no speed, no fix)
```

No fix is offered when the result rounds to 0 or exceeds 9999 km.

## 6. Semantics

**Set expansion** (model-checked in `tla/SetExpansion.tla`):

| Rule | Meaning |
|---|---|
| R1 | A bare set (`12`) needs a default weight. |
| R2 | After the first explicit set (`6x82.5lb`), no bare set may follow. |
| R3 | With `base++step`, the *n*-th bare set (from 0) uses `base + n·step`; every repeated set counts. |
| R4 | `^n` / `*n` emits exactly *n* sets, in order. |

Timed sets follow the same rules, except that a bare hold with no default
load is at body weight (section 4, *Timed sets*).

**Total load** of a set = `initial + sides × written load`. So
`Bench Press +22kg x2 12x5kg` is 12×32kg.

**Mixed units** are kept exact: `63kg++5lbs` gives `63kg`, `63kg+5lbs`,
`63kg+10lbs`… (≈ 63, 65.27, 67.54 kg). Conversions use 1 lb = 0.45359237 kg.

**Body weight**: `bw` marks the load as body weight; explicit sets in a bw
exercise inherit it (`Inclined Crunch bw 12x0kg` is 12×bw). `bw-36kg` is
assisted.

**Super sets** (model-checked in `tla/SupersetGrouping.tla`): a label opens a
group that takes the consecutive items after it while they keep the bullet
of its first member. A blank line, a date, another label or a bullet change
closes it. With `+` members the group is unambiguous; with `-` members it
runs to the next blank line/date/label, so the parser warns.

**Sessions are separated by an empty line.** A line that starts a session
(by the rule in `src/session.rs`, which the merge uses too) must follow an
empty or whitespace-only line, unless it is the first non-blank line of the
file. After the last exercise of a session comes an empty line; only then
another session. A session that follows another line directly still
starts there, with a warning (section 7). Lines that start no session (a
half-typed, impossible or non-ISO date) are not checked.

The sync merge keeps this rule for what it writes: when it adds a session
to one side, moves one into date order, or puts two sessions next to each
other that were not neighbours before, and the first has no empty line
after it, it writes one between them. Sessions neither side touched keep
their separators as written, so a log without empty lines that nobody
changed comes back byte for byte. It never changes bytes inside a line.

**Number limits.** Every number is checked against its limit before it is
converted, so a line of any length parses without overflow. The limit
itself is allowed; anything above it (even `9999.5`) is a parse error whose
span covers the number.

| Number | Limit | Example error |
|---|---|---|
| reps, both sides of `12/11` | 999 | `1000 reps is over the limit of 999` |
| repeat (`^n`, `*n`) | 99 | `100 repeats is over the limit of 99` |
| load: set loads, default loads, `+initial`, `base++step`, `bw±` mass | 9999 | `a load of 10000kg is over the limit of 9999kg` |
| body weight on a date line, in the unit written (grams over 9999 get a fix to kg) | 9999 | `a body weight of 9999.5kg is over the limit of 9999kg` |
| distance | 9999 | `a distance of 10000km is over the limit of 9999km` |
| speed | 9999 | `a speed of 10000km/h is over the limit of 9999km/h` |
| duration minutes / seconds (`20min`, `14m28s`, `11:26`), in cardio and in holds (`60s`) | 9999 | `10000 minutes is over the limit of 9999` |
| a hold's total time | above 0 | `a timed set of 0s holds for no time: a hold lasts at least 1s` |
| range ends (`4.5~15⁰`) | 9999 | `10000 in a range is over the limit of 9999` |
| sets one line expands to | 9999 | `this line expands to 10098 sets, over the limit of 9999` |

Leading zeros don't count (`0999` is 999). The set total is an expansion
error, not a parse error, and is reachable only with more than 101 items
of `^99`. Dates and times of day are not covered here.

## 7. Diagnostics

Errors: any line the grammar rejects (with column; a load with no reps
says so, section 4), numbers over their
limits (section 6), reps and timed sets on one line, a hold of 0, R1/R2 violations, lines with too many sets, and a date
or a time of day that does not exist.
Warnings: a date earlier than the previous session, a session with no empty
line before it, a half-typed date, a date
in another format, an exercise with nothing recorded, a cardio line with no
distance, a super set using `-` bullets.

A session with no empty line before it gets a warning on its date line
with a quick fix, `Add an empty line before 2026-10-29`, that inserts a
newline at the start of the date line. The language server also offers one
action, `Add an empty line before every session`, that inserts every
missing empty line in one edit; it has kind `quickfix` (so editors list it
beside the one-line fix, where a `source.fixAll` action would only run on
request or on save) and is offered for any range while the document has
such a warning. The WebAssembly module reports the warning and its
one-line fix in `diagnostics`; it has no fix-all export.

| Line | Severity | Message |
|---|---|---|
| `2026-02-30` | error | `2026-02-30 is not a real date: February 2026 has 28 days` |
| `2026-13-01` | error | `2026-13-01 is not a real date: months go from 01 to 12` |
| `- Pec Fly 39kg` | error | `39kg has no reps: write 12x39kg or 39kg 12` |
| `25h70~19h15` | error | `25h70 is not a real time of day: hours go from 0 to 23 and minutes from 00 to 59` |
| `2026-10-29` right after `- Row 12x20kg` | warning | `a session starts after an empty line: add one before 2026-10-29` |
| `2026-11-0` | warning | `date is incomplete: write yyyy-mm-dd` |
| `[[2026-11-07` | warning | `date is incomplete: write [[yyyy-mm-dd]]` |
| `07/11/2026` | warning | `dates are written yyyy-mm-dd: did you mean 2026-11-07? please correct the date` |
| `30/02/2026` | warning | `dates are written yyyy-mm-dd, and 30/02/2026 is not a real date: February 2026 has 28 days` |
| `2026-1-07` | warning | `dates are written yyyy-mm-dd: the month in 2026-1-07 has one digit (perhaps 2026-01-07, 2026-10-07, 2026-11-07 or 2026-12-07); please correct the date` |

## 8. Notes on the syntax document

* *Repeating same series*: `12x150lbs^2` expands to 12x150lbs twice; the doc
  lists the second as 12x155lbs.
* *Incremental weight*: the doc renders `63kg++5lbs` set 2 as 145lbs, which
  treats 63kg as 140lbs (machine label). The exact value is 143.9lbs; the
  parser keeps `63kg+5lbs` and leaves rounding to display.
* The sentence "As soon as you define a serie with…" is cut off; rule R2 is
  the reading consistent with every example and with the log.

## 9. Highlighting tokens

`tokenize_line` in `src/tokens.rs` classifies a line for highlighting
(web app, LSP semantic tokens). Unlike the parser it never fails, so a
half-typed line such as `- Bench Press 12x4` or `2026-11-0` is still
coloured. Tokens are 0-based character ranges, sorted, non-overlapping,
never whitespace at either end, and cover every non-whitespace character.
On every line the parser accepts they agree with it (checked against both
logs and by fuzzing in `tests/tokens.rs`).

| Kind (`name()`) | Covers |
|---|---|
| `date` | `2026-11-07`, or a typed prefix of it |
| `date_bracket` | `[[` / `]]` around a date |
| `body_weight` | weigh-in number on a date line: `82.5` in `2026-11-07 82.5kg` (units `kg g lb lbs oz`) |
| `time` | a clock: `18h15` |
| `bullet` | the `-` / `+` of an exercise line |
| `superset_label` | label text between the `_`s |
| `exercise` | the whole exercise name |
| `machine` | the whole machine name after `@` |
| `initial` | number of an initial weight: `22` in `+22kg` |
| `sides` | `x2` / `2x` (any case) |
| `reps` | repetitions: `12` in `12x63kg`, `12 10 8`, both sides of `12/11` |
| `repeat` | count after `^` / `*` |
| `load` | number of a load: `63` in `12x63kg`, `23kg 10`, `bw-36kg` |
| `unit` | `kg lb lbs km mi min m sec s km/h kph mph g oz ⁰ ° º %`, any case, or a typed prefix |
| `bw` | the `bw` keyword (any case) |
| `operator` | `x ^ * ++ + - / : ~ @ _` acting as syntax |
| `distance` / `duration` / `speed` / `range` | numbers of cardio metrics; `duration` also covers the hold time of a timed set (`60` in `bw 60s^2`) |
| `comment` | `;` to end of line |
| `conflict` | a `; conflict YYYY-MM-DD <device>: <line>` line, except the date |
| `conflict_date` | the date of a conflict line |
| `invalid` | anything else |

Numbers and units are separate tokens (`63` load, `kg` unit). The `+` of
`+22kg` is an operator and `22` the initial weight; the `;` belongs to the
comment token. While typing: a bare number is `reps` and the number after
`x` is `load` even without a unit; a lone `x` is `sides`, a lone `b` is
`bw`, and digits alone at the start of a line are a `date` (`18h` makes
them a `time`). A line without a bullet is still tokenized as an exercise.

The language server sends these as semantic tokens: the legend lists
one type per kind, in `TokenKind::ALL` order, named `workout` + the
camel-cased `name()` (`date_bracket` → `workoutDateBracket`), with no
modifiers. Columns are converted to UTF-16 (`⁰`, `º` count 1, an emoji 2).
`invalid` tokens are sent too, as `workoutInvalid`, so an editor can mark
what it cannot read. The Neovim plugin in `editors/nvim/` links
`@lsp.type.workoutX` to a `WorkoutX` highlight group.
