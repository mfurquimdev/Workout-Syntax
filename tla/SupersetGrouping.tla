-------------------------- MODULE SupersetGrouping --------------------------
(***************************************************************************)
(* Assigning exercise lines to super sets.                                *)
(*                                                                         *)
(* Line kinds: "date", "label" (_Name_), "minus" (- item), "plus"         *)
(* (+ item), "blank".                                                      *)
(*                                                                         *)
(* Rule: a label opens a group. The group takes the consecutive items     *)
(* that follow it while they keep the bullet of its first member. It is   *)
(* closed by a blank line, a date, another label, or a bullet change.     *)
(* An item outside any open group is a standalone exercise.               *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets

CONSTANT MaxLen
Kinds == {"date", "label", "minus", "plus", "blank"}
IsItem(k) == k \in {"minus", "plus"}
SeqsUpTo(S, k) == UNION { [1..m -> S] : m \in 0..k }

VARIABLES lines, pos, open, bullet, group, done
vars == <<lines, pos, open, bullet, group, done>>
\* open   : index of the label line of the open group, 0 = none
\* bullet : bullet of the open group's first member, "" = not fixed yet
\* group  : group[i] = label index for item i, 0 = standalone, -1 = not an item

Init ==
    /\ lines \in SeqsUpTo(Kinds, MaxLen)
    /\ pos = 1 /\ open = 0 /\ bullet = "" /\ done = FALSE
    /\ group = [i \in 1..Len(lines) |-> -1]

Step ==
    /\ ~done /\ pos <= Len(lines)
    /\ LET k == lines[pos] IN
       CASE k = "label" ->
              open' = pos /\ bullet' = "" /\ group' = group
         [] k \in {"date", "blank"} ->
              open' = 0 /\ bullet' = "" /\ group' = group
         [] IsItem(k) /\ open # 0 /\ (bullet = "" \/ bullet = k) ->
              open' = open /\ bullet' = k /\ group' = [group EXCEPT ![pos] = open]
         [] IsItem(k) ->   \* no open group, or bullet changed: closes it
              open' = 0 /\ bullet' = "" /\ group' = [group EXCEPT ![pos] = 0]
    /\ pos' = pos + 1
    /\ UNCHANGED <<lines, done>>

Finish == ~done /\ pos = Len(lines) + 1 /\ done' = TRUE
          /\ UNCHANGED <<lines, pos, open, bullet, group>>
Idle   == done /\ UNCHANGED vars

Spec == Init /\ [][Step \/ Finish \/ Idle]_vars /\ WF_vars(Step \/ Finish)

-----------------------------------------------------------------------------
Terminates == <>done

\* every item is assigned exactly once, non-items never
AllAssigned == done => \A i \in 1..Len(lines) :
                  (IsItem(lines[i]) <=> group[i] # -1)

\* a member's group label precedes it, with only same-group items between
Contiguous == done => \A i \in 1..Len(lines) :
    group[i] > 0 => \A j \in (group[i] + 1)..i :
        IsItem(lines[j]) /\ group[j] = group[i]

\* all members of one group share the same bullet
SameBullet == done => \A i, j \in 1..Len(lines) :
    (group[i] > 0 /\ group[i] = group[j]) => lines[i] = lines[j]

\* a label immediately followed by an item always gets that item
LabelGetsFirst == done => \A i \in 1..(Len(lines) - 1) :
    (lines[i] = "label" /\ IsItem(lines[i + 1])) => group[i + 1] = i
=============================================================================
