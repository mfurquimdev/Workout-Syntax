---------------------------- MODULE SetExpansion ----------------------------
(***************************************************************************)
(* Expansion of a workout set list into individual work sets.             *)
(*                                                                         *)
(* A set list is a sequence of items:                                     *)
(*   bare(n)     e.g. "12^3"      reps only; weight comes from the        *)
(*                                default weight (+ k * increment)        *)
(*   explicit(n) e.g. "12x63kg^2" reps and weight given                   *)
(* n is the repeat multiplicity (^n / *n, 1 when absent).                 *)
(*                                                                         *)
(* Rules (from "Workout Syntax.md"):                                       *)
(*   R1  a bare item needs a default weight                               *)
(*   R2  once an explicit item appears, no bare item may follow           *)
(*   R3  every emitted bare set advances the increment step k by one      *)
(*   R4  an item with multiplicity n emits exactly n sets, in order       *)
(*                                                                         *)
(* The model checks, for EVERY list up to MaxLen items, that expansion    *)
(* terminates, and that it ends in "ok" or "error" exactly when the       *)
(* rules say so (completeness), with the right sets (soundness).          *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets

CONSTANTS MaxLen, MaxRep

Kinds  == {"bare", "explicit"}
Items  == [kind : Kinds, n : 1..MaxRep]

\* every sequence of items of length 0..MaxLen
SeqsUpTo(S, k) == UNION { [1..m -> S] : m \in 0..k }

VARIABLES input, hasDefault, pos, out, step, seenExplicit, status
vars == <<input, hasDefault, pos, out, step, seenExplicit, status>>

\* an emitted set: which input item produced it, and its increment step
\* (step = -1 encodes "explicit weight, no increment applied")
OutSet == [src : 1..MaxLen, step : (0..(MaxLen * MaxRep)) \cup {-1}]

TypeOK ==
    /\ input \in SeqsUpTo(Items, MaxLen)
    /\ hasDefault \in BOOLEAN
    /\ pos \in 1..(MaxLen + 1)
    /\ out \in Seq(OutSet)
    /\ step \in 0..(MaxLen * MaxRep)
    /\ seenExplicit \in BOOLEAN
    /\ status \in {"running", "ok", "error"}

Init ==
    /\ input \in SeqsUpTo(Items, MaxLen)
    /\ hasDefault \in BOOLEAN
    /\ pos = 1
    /\ out = << >>
    /\ step = 0
    /\ seenExplicit = FALSE
    /\ status = "running"

Repeat(x, n) == [i \in 1..n |-> x]

\* bare item that may not be emitted: R1 or R2 violated
EmitBareError ==
    /\ status = "running" /\ pos <= Len(input)
    /\ input[pos].kind = "bare"
    /\ (~hasDefault \/ seenExplicit)
    /\ status' = "error"
    /\ UNCHANGED <<input, hasDefault, pos, out, step, seenExplicit>>

EmitBare ==
    /\ status = "running" /\ pos <= Len(input)
    /\ input[pos].kind = "bare"
    /\ hasDefault /\ ~seenExplicit
    /\ LET n == input[pos].n IN
         /\ out' = out \o [i \in 1..n |-> [src |-> pos, step |-> step + i - 1]]
         /\ step' = step + n
    /\ pos' = pos + 1
    /\ UNCHANGED <<input, hasDefault, seenExplicit, status>>

EmitExplicit ==
    /\ status = "running" /\ pos <= Len(input)
    /\ input[pos].kind = "explicit"
    /\ out' = out \o Repeat([src |-> pos, step |-> -1], input[pos].n)
    /\ seenExplicit' = TRUE
    /\ pos' = pos + 1
    /\ UNCHANGED <<input, hasDefault, step, status>>

Finish ==
    /\ status = "running" /\ pos = Len(input) + 1
    /\ status' = "ok"
    /\ UNCHANGED <<input, hasDefault, pos, out, step, seenExplicit>>

Done == status # "running" /\ UNCHANGED vars

Next == EmitBareError \/ EmitBare \/ EmitExplicit \/ Finish \/ Done

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

-----------------------------------------------------------------------------
(* Properties *)

RECURSIVE SumN(_)
SumN(s) == IF s = << >> THEN 0 ELSE Head(s).n + SumN(Tail(s))

\* the rule-based oracle: is this input legal?
Legal ==
    \A i \in 1..Len(input) :
        input[i].kind = "bare" =>
            /\ hasDefault
            /\ \A j \in 1..(i - 1) : input[j].kind # "explicit"

\* Liveness: expansion always terminates
Terminates == <>(status # "running")

\* Completeness: the result is "ok" exactly for legal inputs
Complete == status = "ok" => Legal
Rejects  == status = "error" => ~Legal

\* Soundness of the emitted sets for an ok result
CountOK  == status = "ok" => Len(out) = SumN(input)
OrderOK  == \A i, j \in 1..Len(out) : i < j => out[i].src <= out[j].src
StepsOK  == \A i \in 1..Len(out) :
               out[i].step # -1 =>
                 out[i].step = Cardinality({ j \in 1..(i - 1) : out[j].step # -1 })
ExplicitLastOK ==
    \A i, j \in 1..Len(out) :
        (out[i].step = -1 /\ i < j) => out[j].step = -1

\* every legal input actually reaches "ok" (no legal input gets stuck/rejected)
LegalAccepted == <>(Legal => status = "ok")
=============================================================================
