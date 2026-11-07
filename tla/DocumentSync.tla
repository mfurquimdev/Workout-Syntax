---------------------------- MODULE DocumentSync ----------------------------
(***************************************************************************)
(* The language server's document store and diagnostics publishing.        *)
(*                                                                         *)
(* The client opens, changes and closes documents; each notification       *)
(* reaches the server in order. On every open/change the server re-parses  *)
(* the full text and publishes diagnostics tagged with the version; on     *)
(* close it forgets the text and publishes an empty list.                  *)
(*                                                                         *)
(* Checked: whenever the server has drained its inbox, the client's view   *)
(* of every document's diagnostics matches the latest text it sent         *)
(* (open docs) or is empty (closed docs); a publish never carries a        *)
(* version older than one already published; nothing is kept for           *)
(* closed documents.                                                       *)
(***************************************************************************)
EXTENDS Naturals, Sequences

CONSTANTS Docs, Texts, MaxVersion
NoText == "none"

\* Diag(t) is "the diagnostics of text t"; abstractly, the text itself
Diag(t) == t

VARIABLES
    clientText,   \* doc -> text the client currently has (NoText = closed)
    clientVer,    \* doc -> version counter on the client
    inbox,        \* notifications in flight, client -> server (FIFO)
    store,        \* server: doc -> [text, ver] or NoText
    shown,        \* client: doc -> diagnostics it displays ({} = none)
    shownVer      \* client: doc -> version of the shown diagnostics
vars == <<clientText, clientVer, inbox, store, shown, shownVer>>

Msg == [kind : {"open", "change", "close"}, doc : Docs, text : Texts \cup {NoText}, ver : 0..MaxVersion]

Init ==
    /\ clientText = [d \in Docs |-> NoText]
    /\ clientVer  = [d \in Docs |-> 0]
    /\ inbox = << >>
    /\ store = [d \in Docs |-> NoText]
    /\ shown = [d \in Docs |-> "empty"]
    /\ shownVer = [d \in Docs |-> 0]

\* ---- client actions ----
Open(d, t) ==
    /\ clientText[d] = NoText /\ clientVer[d] < MaxVersion
    /\ clientText' = [clientText EXCEPT ![d] = t]
    /\ clientVer'  = [clientVer EXCEPT ![d] = @ + 1]
    /\ inbox' = Append(inbox, [kind |-> "open", doc |-> d, text |-> t, ver |-> clientVer[d] + 1])
    /\ UNCHANGED <<store, shown, shownVer>>

Change(d, t) ==
    /\ clientText[d] # NoText /\ clientVer[d] < MaxVersion
    /\ clientText' = [clientText EXCEPT ![d] = t]
    /\ clientVer'  = [clientVer EXCEPT ![d] = @ + 1]
    /\ inbox' = Append(inbox, [kind |-> "change", doc |-> d, text |-> t, ver |-> clientVer[d] + 1])
    /\ UNCHANGED <<store, shown, shownVer>>

Close(d) ==
    /\ clientText[d] # NoText
    /\ clientText' = [clientText EXCEPT ![d] = NoText]
    /\ inbox' = Append(inbox, [kind |-> "close", doc |-> d, text |-> NoText, ver |-> clientVer[d]])
    /\ UNCHANGED <<clientVer, store, shown, shownVer>>

\* ---- server: handle the next notification and publish ----
Handle ==
    /\ inbox # << >>
    /\ LET m == Head(inbox) IN
       /\ inbox' = Tail(inbox)
       /\ IF m.kind = "close"
            THEN /\ store' = [store EXCEPT ![m.doc] = NoText]
                 /\ shown' = [shown EXCEPT ![m.doc] = "empty"]
                 /\ shownVer' = [shownVer EXCEPT ![m.doc] = m.ver]
            ELSE /\ store' = [store EXCEPT ![m.doc] = [text |-> m.text, ver |-> m.ver]]
                 /\ shown' = [shown EXCEPT ![m.doc] = Diag(m.text)]
                 /\ shownVer' = [shownVer EXCEPT ![m.doc] = m.ver]
    /\ UNCHANGED <<clientText, clientVer>>

Next ==
    \/ \E d \in Docs, t \in Texts : Open(d, t) \/ Change(d, t)
    \/ \E d \in Docs : Close(d)
    \/ Handle

Spec == Init /\ [][Next]_vars /\ WF_vars(Handle)

-----------------------------------------------------------------------------
\* When the server is idle, what the client shows is exactly right.
Consistent ==
    inbox = << >> =>
        \A d \in Docs :
            IF clientText[d] = NoText
              THEN shown[d] = "empty" /\ store[d] = NoText
              ELSE shown[d] = Diag(clientText[d]) /\ store[d].text = clientText[d]
                   /\ shownVer[d] = clientVer[d]

\* Published versions never go backwards.
Monotonic == [][\A d \in Docs : shownVer'[d] >= shownVer[d]]_vars

\* Every notification is eventually handled (no message lost).
Drains == []<>(inbox = << >>)
=============================================================================
