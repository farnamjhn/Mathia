# 🧠 Mathematical Logic Midterm Study Guide: Chapters 1 & 2

**Source**: [A. G. Hamilton, *Logic for Mathematicians* (Revised Edition, Cambridge University Press)](https://www.cambridge.org/gb/universitypress/subjects/mathematics/logic-categories-and-sets/logic-mathematicians-2nd-edition) — **Chapter 1 (Informal Statement Calculus)** & **Chapter 2 (Formal Statement Calculus)** ([Course Notes Reference](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf))

> 🎯 **How to Use This Guide for Your Midterm**:
> * **Chapter 1 (Informal Statement Calculus)** is the **semantic** world: we care about *truth values* ($\text{T}$ and $\text{F}$), *truth tables*, *logical equivalences*, *normal forms (DNF/CNF)*, and *adequate sets of connectives*.
> * **Chapter 2 (Formal Statement Calculus — System $L$)** is the **syntactic** world: we pretend we don't know what "true" or "false" means and play a rigorous symbol-manipulation game using only **3 Axiom Schemes ((L1), (L2), (L3))** and **1 Rule of Inference (Modus Ponens)**.
> * **Section 2.2 (The Adequacy & Soundness Theorems)** builds the **bridge** between Chapter 1 and Chapter 2: proving that the semantic tautologies ($\models_L A$) and the syntactic theorems ($\vdash_L A$) are the exact same formulas!

# PART I — Chapter 1: Informal Statement Calculus

## 1.1 Statements and Connectives

### 🌟 Core Idea: Form Over Meaning
Mathematical logic is the study of **deduction** ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)). Whether a mathematical deduction is valid depends on the **structural form** of the statements involved, not on the specific objects they talk about. Because ordinary human language is full of ambiguity, we introduce a symbolic language to make logical form crystal clear ([Propositional Formula Overview](https://en.wikipedia.org/wiki/Propositional_formula)).

* **Simple Declarative Sentence**: A sentence with a single **subject** and **predicate** that makes a definite assertion (e.g., *"Socrates is a man"*, *"$5$ is a prime number"*) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
* **Compound Sentence**: A sentence formed by combining simple sentences using logical **connectives** (*"not"*, *"and"*, *"or"*, *"if ... then ..."*, *"if and only if"*) ([Propositional Logic Reference](https://en.wikipedia.org/wiki/Propositional_logic)).
* **Principle of Bivalence (Fundamental Assumption)**: Every statement we study is assumed to be either **true** ($\text{T}$) or **false** ($\text{F}$), and **never both** ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)). Questions, commands, exclamations, and self-referential paradoxes (like *"This statement is false"*) are excluded from the statement calculus.
* **Notation for Particular Statements**: Capital letters $A, B, C, \dots$ denote particular statements (or arbitrary statement forms) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

### 📘 Definition 1.1: The Five Basic Logical Connectives
Given statements $A$ and $B$, we form compound statements using five truth-functional connectives ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Propositional Logic Reference](https://en.wikipedia.org/wiki/Propositional_logic)):

| Connective Name | Symbol | Symbolic Form | Standard Reading | Equivalent English Phrasings on Exams | Source |
| :--- | :---: | :---: | :--- | :--- | :--- |
| **Negation** | $\sim$ (or $\neg$) | $(\sim A)$ | "not $A$" | "It is not the case that $A$", "$A$ is false" | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| **Conjunction** | $\wedge$ | $(A \wedge B)$ | "$A$ and $B$" | "$A$ but $B$", "$A$ although $B$", "both $A$ and $B$" | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| **Disjunction** | $\vee$ | $(A \vee B)$ | "$A$ or $B$" | "$A$ or $B$ or both" (*inclusive* or), "$A$ unless $B$" ($\equiv (\sim B) \to A$) | [Propositional Logic](https://en.wikipedia.org/wiki/Propositional_logic) |
| **Conditional** (*Implication*) | $\to$ | $(A \to B)$ | "if $A$ then $B$" | "$A$ implies $B$", "$A$ only if $B$", "$B$ if $A$", "$A$ is sufficient for $B$", "$B$ is necessary for $A$" | [Material Conditional](https://math.stackexchange.com/questions/2125136/on-a-characterization-of-the-material-conditional-to) |
| **Biconditional** (*Equivalence*) | $\leftrightarrow$ | $(A \leftrightarrow B)$ | "$A$ if and only if $B$" | "$A$ is equivalent to $B$", "$A$ is necessary and sufficient for $B$" | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |

> ⚠️ **Exam Watchout — Translating English into Symbols**:
> 1. **Inclusive vs. Exclusive "Or"**: In mathematical logic, $(A \vee B)$ is **always inclusive** (true when $A$ is true, when $B$ is true, or when *both* are true). If an exam problem asks for **exclusive or** (*"either $A$ or $B$, but not both"*), write $((A \vee B) \wedge (\sim(A \wedge B)))$ or $(\sim(A \leftrightarrow B))$ ([Propositional Logic Reference](https://en.wikipedia.org/wiki/Propositional_logic)).
> 2. **"If" vs. "Only If"**:
>    * *"$A$ if $B$"* means $B$ is the condition: **$(B \to A)$**.
>    * *"$A$ only if $B$"* means $A$ cannot happen without $B$: **$(A \to B)$**.
>    * *"$A$ if and only if $B$"* combines both: **$((A \to B) \wedge (B \to A))$**, written **$(A \leftrightarrow B)$**.
> 3. **Why Brackets Matter**: The English sentence *"Either $A$ is false and $B$ is true, or $C$ is true"* is ambiguous without parentheses. Depending on placement, it could mean $(((\sim A) \wedge B) \vee C)$, $((\sim(A \wedge B)) \vee C)$, or $((\sim A) \wedge (B \vee C))$, each having a different truth table ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

> 📝 **My Study Notes (Section 1.1)**:
> * *Key takeaway to remember:*

## 1.2 Truth Functions and Truth Tables

### 📘 Definition 1.2: Truth Values and $n$-Place Truth Functions
Let $\{\text{T}, \text{F}\}$ be the set of **truth values** ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)). An **$n$-place truth function** (a truth function of $n$ variables) is a function

$$f : \{\text{T}, \text{F}\}^n \to \{\text{T}, \text{F}\}.$$

> 💡 **Combinatorial Fact for Exams**:
> Since an $n$-tuple $(x_1, x_2, \dots, x_n) \in \{\text{T}, \text{F}\}^n$ has $2^n$ possible input combinations (the $2^n$ rows of a truth table), and each row can independently output either $\text{T}$ or $\text{F}$, the total number of distinct $n$-place truth functions is:
>
> $$\text{Number of } n\text{-place truth functions} = 2^{(2^n)}$$
>
> * For $n = 1$: $2^{(2^1)} = 4$ unary truth functions ($\text{T}$, $\text{F}$, $p$, $\sim p$) ([Statement Logic Combinatorics](https://mathscitech.org/articles/prop-logic)).
> * For $n = 2$: $2^{(2^2)} = 16$ binary truth functions ([Statement Logic Combinatorics](https://mathscitech.org/articles/prop-logic)).
> * For $n = 3$: $2^{(2^3)} = 256$ ternary truth functions ([Statement Logic Combinatorics](https://mathscitech.org/articles/prop-logic)).

### 📘 Definition 1.3: Truth Tables of the Five Basic Connectives
Each basic connective defines a unary ($f_{\sim}$) or binary ($f_{\wedge}, f_{\vee}, f_{\to}, f_{\leftrightarrow}$) truth function given by the following master truth table ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)):

| $p$ | $q$ | $(\sim p)$ | $(p \wedge q)$ | $(p \vee q)$ | $(p \to q)$ | $(p \leftrightarrow q)$ | Source |
| :---: | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| $\text{T}$ | $\text{T}$ | $\text{F}$ | $\text{T}$ | $\text{T}$ | $\text{T}$ | $\text{T}$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| $\text{T}$ | $\text{F}$ | $\text{F}$ | $\text{F}$ | $\text{T}$ | $\text{F}$ | $\text{F}$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| $\text{F}$ | $\text{T}$ | $\text{T}$ | $\text{F}$ | $\text{T}$ | $\text{T}$ | $\text{F}$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| $\text{F}$ | $\text{F}$ | $\text{T}$ | $\text{F}$ | $\text{F}$ | $\text{T}$ | $\text{T}$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |

> 🧠 **Memory Rules for Fast Truth-Table Calculation**:
> * **$\sim p$**: Flips $\text{T} \leftrightarrow \text{F}$.
> * **$p \wedge q$**: $\text{T}$ **only** when **both** $p$ and $q$ are $\text{T}$.
> * **$p \vee q$**: $\text{F}$ **only** when **both** $p$ and $q$ are $\text{F}$.
> * **$p \to q$**: $\text{F}$ **only** when $p$ is $\text{T}$ and $q$ is $\text{F}$ (*a true premise cannot lead to a false conclusion*). Whenever $p = \text{F}$ OR $q = \text{T}$, $(p \to q)$ is immediately $\text{T}$!
> * **$p \leftrightarrow q$**: $\text{T}$ **only** when $p$ and $q$ have the **same** truth value.

> 💡 **Why is $(p \to q)$ True When $p$ is False? (Hamilton's Explanation)**:
> Students often wonder why $\text{F} \to \text{T}$ and $\text{F} \to \text{F}$ are defined as $\text{T}$.
> 1. In mathematics, asserting $A \to B$ means: *"If $A$ is true, then $B$ must be true; if $A$ is false, nothing is claimed about $B$."* The only case that violates the guarantee $A \to B$ is when $A$ is $\text{T}$ and $B$ is $\text{F}$ ([Material Conditional Discussion](https://math.stackexchange.com/questions/2125136/on-a-characterization-of-the-material-conditional-to)).
> 2. Consider the universal mathematical truth: *"For all $x \in \mathbb{R}$, if $x > 2$ then $x^2 > 4$."* This must hold for every real number $x$:
>    * For $x = 3$: $3 > 2$ ($\text{T}$) $\to 9 > 4$ ($\text{T}$) is $\text{T}$.
>    * For $x = 1$: $1 > 2$ ($\text{F}$) $\to 1 > 4$ ($\text{F}$) must be $\text{T}$.
>    * For $x = -3$: $-3 > 2$ ($\text{F}$) $\to 9 > 4$ ($\text{T}$) must be $\text{T}$.
> 3. If the third row ($\text{F}, \text{T}$) were $\text{F}$, then $(p \to q)$ would have the exact same truth table as $(p \leftrightarrow q)$, destroying the distinction between *"if $p$ then $q$"* and *"$p$ if and only if $q$"* ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

### 📘 Definition 1.4: Statement Variables and Statement Forms (Recursive Definition)
* **Statement Variables**: Lowercase letters $p, q, r, \dots$ (or $p_1, p_2, p_3, \dots$) are called **statement variables**. They are formal placeholders into which concrete statements or truth values ($\text{T}, \text{F}$) can be substituted ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
* **Statement Form**: A **statement form** in the statement variables $p_1, p_2, \dots, p_n$ is a finite string of symbols defined inductively by three clauses ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Propositional Calculus Reference](https://educational-sciences-journal.lacts.org.ly/esj/index.php/esj/article/download/216/190/372)):
  1. **Base Clause**: Every statement variable $p_i$ is a statement form.
  2. **Inductive Clause**: If $A$ and $B$ are statement forms, then $(\sim A)$, $(A \wedge B)$, $(A \vee B)$, $(A \to B)$, and $(A \leftrightarrow B)$ are statement forms.
  3. **Extremal Clause**: Nothing else is a statement form unless it is obtained by a finite number of applications of clauses (1) and (2).

* **Key Property (Statement Forms $\to$ Truth Functions)**:
  * Every statement form $A$ in $n$ variables determines a **unique** $n$-place truth function $f_A : \{\text{T}, \text{F}\}^n \to \{\text{T}, \text{F}\}$ ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
  * Conversely, a single truth function is determined by **infinitely many** different statement forms (for example, $p$, $\sim(\sim p)$, $(p \wedge p)$, and $(p \vee (q \wedge (\sim q)))$ all determine the exact same truth function!) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
* **Notational Convention**: We routinely omit the **outermost pair of brackets** of a statement form, writing $p \to (q \vee r)$ as shorthand for $(p \to (q \vee r))$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).

### 📘 Definitions 1.5–1.8: Tautology, Contradiction, Logical Implication, and Logical Equivalence

1. **Definition 1.5 (Tautology)**:
   A statement form $A$ is a **tautology** if $A$ takes the truth value $\text{T}$ for **every** possible assignment of truth values ($\text{T}$ or $\text{F}$) to its statement variables (the final column of its truth table is all $\text{T}$'s) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
   * *Classic Example*: $(p \vee (\sim p))$ (*Law of Excluded Middle*) and $((p \to q) \leftrightarrow ((\sim p) \vee q))$.

2. **Definition 1.6 (Contradiction)**:
   A statement form $A$ is a **contradiction** if $A$ takes the truth value $\text{F}$ for **every** possible assignment of truth values to its statement variables (the final column of its truth table is all $\text{F}$'s) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
   * *Classic Example*: $(p \wedge (\sim p))$.
   * *Duality Note*: $A$ is a tautology $\iff (\sim A)$ is a contradiction; $A$ is a contradiction $\iff (\sim A)$ is a tautology ([Features of Propositional Logic](https://www.m-hikari.com/pms/pms-2021/pms-1-2021/p/alodhariPMS1-2021.pdf)). A statement form that is neither a tautology nor a contradiction is called *contingent*.

3. **Definition 1.7 (Logical Implication)**:
   A statement form $A$ **logically implies** a statement form $B$ if and only if the statement form $(A \to B)$ is a **tautology** ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
   * *Equivalent Formulation*: There is **no** assignment of truth values to the statement variables occurring in $A$ and $B$ under which $A$ has the value $\text{T}$ and $B$ has the value $\text{F}$ (every row where $A$ is $\text{T}$ also has $B$ equal to $\text{T}$) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
   * *Example*: $(p \wedge q)$ logically implies $p$ because $((p \wedge q) \to p)$ is a tautology ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

4. **Definition 1.8 (Logical Equivalence)**:
   Two statement forms $A$ and $B$ are **logically equivalent** (often written $A \equiv B$ in study notes) if and only if the statement form $(A \leftrightarrow B)$ is a **tautology** ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
   * *Equivalent Formulation*: Under **every** truth-value assignment to the statement variables appearing in $A$ and $B$, $A$ and $B$ take the **exact same truth value** (i.e., $A$ and $B$ determine the same truth function) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
   * *Example*: $\sim(p \wedge q)$ is logically equivalent to $((\sim p) \vee (\sim q))$ ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

> 💡 **Hamilton's Fast Compact Truth-Table Method (p. 9)**:
> Instead of making separate columns for every subformula, you can write the truth values directly underneath each variable and connective, numbering the steps $[1], [2], [3], \dots$ from the innermost brackets outward to the main connective ([Hamilton Compact Truth Table Method](https://mathscitech.org/articles/prop-logic))!

> 📝 **My Study Notes (Section 1.2)**:
> * *Key takeaway to remember:*

## 1.3 Rules for Manipulation and Substitution

Why do we need manipulation and substitution rules? Because building a $2^n$-row truth table becomes impractical when $n$ is large! The following propositions let us generate new tautologies and equivalences **algebraically** from ones we already know ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Hamilton Compact Truth Table Method](https://mathscitech.org/articles/prop-logic)).

### 🏆 Proposition 1.1: Modus Ponens for Tautologies
> **Statement**: If $A$ and $(A \to B)$ are tautologies, then $B$ is a tautology ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Propositional Calculus Reference](https://educational-sciences-journal.lacts.org.ly/esj/index.php/esj/article/download/216/190/372)).

* **Proof**:
  Suppose $A$ and $(A \to B)$ are tautologies, and suppose for contradiction that $B$ is **not** a tautology. Then there exists an assignment of truth values to the statement variables appearing in $A$ or $B$ that gives $B$ the value $\text{F}$. Because $A$ is a tautology, that same assignment must give $A$ the value $\text{T}$. By the truth table of $\to$, when $A$ has value $\text{T}$ and $B$ has value $\text{F}$, $(A \to B)$ has the value $\text{F}$. This contradicts the hypothesis that $(A \to B)$ is a tautology. Hence $B$ must be a tautology ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)). $\blacksquare$

### 🏆 Proposition 1.2: Uniform Substitution in a Tautology
> **Statement**: Let $A$ be a statement form in which the statement variables $p_1, p_2, \dots, p_n$ appear, and let $A_1, A_2, \dots, A_n$ be any statement forms. If $A$ is a tautology, then the statement form $B$, obtained from $A$ by replacing **each occurrence** of $p_i$ by $A_i$ ($1 \le i \le n$) throughout, is also a tautology ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Logic of Antinomies Reference](https://projecteuclid.org/journals/notre-dame-journal-of-formal-logic/volume-16/issue-1/Logic-of-antinomies/10.1305/ndjfl/1093891610.pdf)).

* **Proof**:
  Let any assignment of truth values be given to the statement variables occurring in $A_1, \dots, A_n$ (which are the variables of $B$). Under this assignment, each statement form $A_i$ takes a definite truth value $x_i \in \{\text{T}, \text{F}\}$ ($1 \le i \le n$). Because $B$ is built from $A_1, \dots, A_n$ in the exact same way that $A$ is built from $p_1, \dots, p_n$, the truth value of $B$ under the given assignment is equal to the truth value of $A$ when $p_1, \dots, p_n$ are assigned the truth values $x_1, \dots, x_n$. Since $A$ is a tautology, $A$ takes the value $\text{T}$ for every assignment of truth values to $p_1, \dots, p_n$. Therefore $B$ takes the value $\text{T}$ for every assignment of truth values to its statement variables, so $B$ is a tautology ([Logic of Antinomies Reference](https://projecteuclid.org/journals/notre-dame-journal-of-formal-logic/volume-16/issue-1/Logic-of-antinomies/10.1305/ndjfl/1093891610.pdf)). $\blacksquare$

### 🏆 Proposition 1.3: Rule of Replacement (Substitution of Equivalent Subformulas)
> **Statement**: Let $B$ be a statement form in which $A_1$ appears as a part (subformula), and let $C$ be the statement form obtained from $B$ by replacing **some or all** occurrences of $A_1$ in $B$ by a statement form $A_2$. If $A_1$ and $A_2$ are logically equivalent (i.e., $(A_1 \leftrightarrow A_2)$ is a tautology), then $B$ and $C$ are logically equivalent (i.e., $(B \leftrightarrow C)$ is a tautology) ([Logic of Antinomies Reference](https://projecteuclid.org/journals/notre-dame-journal-of-formal-logic/volume-16/issue-1/Logic-of-antinomies/10.1305/ndjfl/1093891610.pdf); [Propositional Calculus Reference](https://educational-sciences-journal.lacts.org.ly/esj/index.php/esj/article/download/216/190/372)).

* **Proof**:
  Consider any assignment of truth values to the statement variables appearing in $B$ and $C$. Because $(A_1 \leftrightarrow A_2)$ is a tautology, $A_1$ and $A_2$ take the exact same truth value under this assignment. Since the truth value of a compound statement form depends solely on the truth values of its subformulas, replacing one or more occurrences of $A_1$ inside $B$ by $A_2$ leaves the overall truth value unchanged. Thus $B$ and $C$ have the same truth value under every truth-value assignment, which means $(B \leftrightarrow C)$ is a tautology ([Logic of Antinomies Reference](https://projecteuclid.org/journals/notre-dame-journal-of-formal-logic/volume-16/issue-1/Logic-of-antinomies/10.1305/ndjfl/1093891610.pdf)). $\blacksquare$

### 🏆 Corollary 1.4: Preservation of Tautologies Under Replacement
> **Statement**: With $B, C, A_1, A_2$ as in Proposition 1.3, if $A_1$ and $A_2$ are logically equivalent and $B$ is a tautology, then $C$ is a tautology ([Propositional Calculus Reference](https://educational-sciences-journal.lacts.org.ly/esj/index.php/esj/article/download/216/190/372)).

* **Proof**:
  By Proposition 1.3, $(B \leftrightarrow C)$ is a tautology, so $C$ has the same truth value as $B$ on every row. Since $B$ is always $\text{T}$, $C$ is always $\text{T}$ ([Propositional Calculus Reference](https://educational-sciences-journal.lacts.org.ly/esj/index.php/esj/article/download/216/190/372)). $\blacksquare$

> ⚠️ **Crucial Exam Comparison — Uniform Substitution (Prop. 1.2) vs. Replacement (Prop. 1.3)**:

| Feature | Uniform Substitution (Proposition 1.2) | Subformula Replacement (Proposition 1.3) |
| :--- | :--- | :--- |
| **What is replaced?** | A **statement variable** $p_i$ | Any **subformula** $A_1$ |
| **Replaced by what?** | **Any** arbitrary statement form $A_i$ | Only a **logically equivalent** form $A_2$ ($A_1 \equiv A_2$) |
| **How many occurrences?** | Must replace **ALL** occurrences of $p_i$ | May replace **SOME or ALL** occurrences of $A_1$ |
| **What is preserved?** | Preserves **tautologies** ($A$ tautology $\implies B$ tautology) | Preserves **truth tables** ($B \equiv C$ for *any* $B$) |

### 📋 Master Table of Fundamental Logical Equivalences (Algebra of Statement Forms)
For any statement forms $A, B, C$, the following biconditionals are tautologies (so the left and right sides can be freely replaced for one another inside any formula by Proposition 1.3) ([Sentential Logic Reference](https://euclid.colorado.edu/~monkd/logic/logic01.pdf); [Propositional Formula Overview](https://en.wikipedia.org/wiki/Propositional_formula)):

| Law Name | Logical Equivalence (Tautological Biconditional) | Source |
| :--- | :--- | :--- |
| **Double Negation** | $\sim(\sim A) \leftrightarrow A$ | [Sentential Logic](https://euclid.colorado.edu/~monkd/logic/logic01.pdf) |
| **Idempotent Laws** | $(A \wedge A) \leftrightarrow A$ <br> $(A \vee A) \leftrightarrow A$ | [Sentential Logic](https://euclid.colorado.edu/~monkd/logic/logic01.pdf) |
| **Commutative Laws** | $(A \wedge B) \leftrightarrow (B \wedge A)$ <br> $(A \vee B) \leftrightarrow (B \vee A)$ <br> $(A \leftrightarrow B) \leftrightarrow (B \leftrightarrow A)$ | [Sentential Logic](https://euclid.colorado.edu/~monkd/logic/logic01.pdf) |
| **Associative Laws** | $((A \wedge B) \wedge C) \leftrightarrow (A \wedge (B \wedge C))$ <br> $((A \vee B) \vee C) \leftrightarrow (A \vee (B \vee C))$ <br> $((A \leftrightarrow B) \leftrightarrow C) \leftrightarrow (A \leftrightarrow (B \leftrightarrow C))$ | [Sentential Logic](https://euclid.colorado.edu/~monkd/logic/logic01.pdf) |
| **Distributive Laws** | $(A \wedge (B \vee C)) \leftrightarrow ((A \wedge B) \vee (A \wedge C))$ <br> $(A \vee (B \wedge C)) \leftrightarrow ((A \vee B) \wedge (A \vee C))$ | [Sentential Logic](https://euclid.colorado.edu/~monkd/logic/logic01.pdf) |
| **De Morgan's Laws** | $\sim(A \wedge B) \leftrightarrow ((\sim A) \vee (\sim B))$ <br> $\sim(A \vee B) \leftrightarrow ((\sim A) \wedge (\sim B))$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| **Material Implication (Eliminating $\to$)** | $(A \to B) \leftrightarrow ((\sim A) \vee B)$ <br> $\sim(A \to B) \leftrightarrow (A \wedge (\sim B))$ | [Sentential Logic](https://euclid.colorado.edu/~monkd/logic/logic01.pdf) |
| **Contraposition** | $(A \to B) \leftrightarrow ((\sim B) \to (\sim A))$ | [Sentential Logic](https://euclid.colorado.edu/~monkd/logic/logic01.pdf) |
| **Exportation / Importation** | $((A \wedge B) \to C) \leftrightarrow (A \to (B \to C))$ | [Sentential Logic](https://euclid.colorado.edu/~monkd/logic/logic01.pdf) |
| **Biconditional (Eliminating $\leftrightarrow$)** | $(A \leftrightarrow B) \leftrightarrow ((A \to B) \wedge (B \to A))$ <br> $(A \leftrightarrow B) \leftrightarrow ((A \wedge B) \vee ((\sim A) \wedge (\sim B)))$ | [Propositional Formula](https://en.wikipedia.org/wiki/Propositional_formula) |
| **Absorption Laws** | $(A \wedge (A \vee B)) \leftrightarrow A$ <br> $(A \vee (A \wedge B)) \leftrightarrow A$ | [Propositional Formula](https://en.wikipedia.org/wiki/Propositional_formula) |
| **Tautology ($\mathbf{t}$) & Contradiction ($\mathbf{f}$) Laws** | $(A \vee (\sim A)) \leftrightarrow \mathbf{t}, \quad (A \wedge (\sim A)) \leftrightarrow \mathbf{f}$ <br> $(A \wedge \mathbf{t}) \leftrightarrow A, \quad (A \vee \mathbf{f}) \leftrightarrow A$ <br> $(A \vee \mathbf{t}) \leftrightarrow \mathbf{t}, \quad (A \wedge \mathbf{f}) \leftrightarrow \mathbf{f}$ | [Propositional Formula](https://en.wikipedia.org/wiki/Propositional_formula) |

> 📝 **My Study Notes (Section 1.3)**:
> * *Key takeaway to remember:*

## 1.4 Normal Forms (DNF & CNF)

### 📘 Definition 1.9: Restricted Statement Form
A statement form is called a **restricted statement form** if it contains **only** the connectives $\sim$, $\wedge$, and $\vee$ (no $\to$ and no $\leftrightarrow$) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

### 🏆 Proposition 1.5: Expressibility by Restricted Statement Forms
> **Statement**: Every truth function is the truth function determined by a restricted statement form ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)). *(Equivalently, every statement form is logically equivalent to a restricted statement form.)*

* **Constructive Proof (The "Read the $\text{T}$-Rows" Algorithm)**:
  Let $f : \{\text{T}, \text{F}\}^n \to \{\text{T}, \text{F}\}$ be any truth function in the variables $p_1, p_2, \dots, p_n$, given by its $2^n$-row truth table ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)):
  1. **Case 1 ($f$ is $\text{F}$ on every row — a contradiction)**:
     Then $f$ is determined by the restricted statement form $(p_1 \wedge (\sim p_1)) \wedge p_2 \wedge \dots \wedge p_n$ (or simply $(p_1 \wedge (\sim p_1))$).
  2. **Case 2 ($f$ takes the value $\text{T}$ on $k \ge 1$ rows, say rows $i_1, i_2, \dots, i_k$)**:
     For each $\text{T}$-row $i_r$ ($1 \le r \le k$) and each variable $p_j$ ($1 \le j \le n$), define the literal $Q_{rj}$ by:

     $$Q_{rj} = \begin{cases} p_j & \text{if } p_j \text{ has value } \text{T} \text{ in row } i_r, \\ \sim p_j & \text{if } p_j \text{ has value } \text{F} \text{ in row } i_r. \end{cases}$$

     Form the conjunction for row $i_r$:

     $$C_r = (Q_{r1} \wedge Q_{r2} \wedge \dots \wedge Q_{rn}).$$

     Notice that $C_r$ takes the value $\text{T}$ on row $i_r$ (where every $Q_{rj}$ is $\text{T}$) and takes the value $\text{F}$ on every other row (because any other row differs from row $i_r$ in at least one variable $p_j$, making that $Q_{rj}$ false).
     Now take the disjunction of $C_1, C_2, \dots, C_k$:

     $$A = (Q_{11} \wedge \dots \wedge Q_{1n}) \vee (Q_{21} \wedge \dots \wedge Q_{2n}) \vee \dots \vee (Q_{k1} \wedge \dots \wedge Q_{kn}).$$

     Then $A$ is a restricted statement form that takes the value $\text{T}$ on rows $i_1, \dots, i_k$ and $\text{F}$ elsewhere, so $A$ determines $f$ ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)). $\blacksquare$

### 🏆 Corollary 1.6: Disjunctive Normal Form (DNF)
> **Statement**: Every statement form which is **not a contradiction** is logically equivalent to a restricted statement form of the form:
>
> $$(Q_{11} \wedge Q_{12} \wedge \dots \wedge Q_{1m_1}) \vee (Q_{21} \wedge Q_{22} \wedge \dots \wedge Q_{2m_2}) \vee \dots \vee (Q_{k1} \wedge Q_{k2} \wedge \dots \wedge Q_{km_k}),$$
>
> where each $Q_{ij}$ is either a statement variable or the negation of a statement variable. This form is called a **disjunctive normal form (DNF)** ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

* **Proof**: If $A$ is not a contradiction, its truth table has at least one row with value $\text{T}$, so Case 2 of Proposition 1.5 produces a logically equivalent formula in DNF (with $m_1 = \dots = m_k = n$) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)). $\blacksquare$

### 🏆 Corollary 1.7: Conjunctive Normal Form (CNF)
> **Statement**: Every statement form which is **not a tautology** is logically equivalent to a restricted statement form of the form:
>
> $$(Q_{11} \vee Q_{12} \vee \dots \vee Q_{1m_1}) \wedge (Q_{21} \vee Q_{22} \vee \dots \vee Q_{2m_2}) \wedge \dots \wedge (Q_{k1} \vee Q_{k2} \vee \dots \vee Q_{km_k}),$$
>
> where each $Q_{ij}$ is either a statement variable or the negation of a statement variable. This form is called a **conjunctive normal form (CNF)** ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

* **Proof**:
  Let $A$ be a statement form which is not a tautology. Then $(\sim A)$ is **not a contradiction**. By Corollary 1.6, $(\sim A)$ is logically equivalent to a statement form in DNF:

  $$(\sim A) \equiv (R_{11} \wedge \dots \wedge R_{1m_1}) \vee \dots \vee (R_{k1} \wedge \dots \wedge R_{km_k}),$$

  where each $R_{ij}$ is a variable or the negation of a variable. Since $A \equiv \sim(\sim A)$, we negate both sides and apply De Morgan's Laws ($\sim(X \vee Y) \equiv (\sim X) \wedge (\sim Y)$ and $\sim(X \wedge Y) \equiv (\sim X) \vee (\sim Y)$) along with Double Negation ($\sim(\sim p) \equiv p$):

  $$A \equiv \sim(\sim A) \equiv ((\sim R_{11}) \vee \dots \vee (\sim R_{1m_1})) \wedge \dots \wedge ((\sim R_{k1}) \vee \dots \vee (\sim R_{km_k})),$$

  which is in CNF ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)). $\blacksquare$

> 🛠️ **Two Exam Recipes to Find DNF & CNF**:
> 1. **Recipe A — From the Truth Table (Full/Canonical Normal Form)**:
>    * **For DNF (Disjunction of $\wedge$-clauses)**: Look at the rows where $A = \text{T}$. Write a conjunction $(\dots \wedge \dots)$ for each $\text{T}$-row matching $\text{T} \mapsto p_j$ and $\text{F} \mapsto \sim p_j$, then join them with $\vee$ ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
>      * *Example*: If $f(p, q, r) = \text{T}$ on $(\text{T},\text{T},\text{T})$, $(\text{T},\text{T},\text{F})$, and $(\text{F},\text{F},\text{F})$, then:
>
>        $$\text{DNF} = (p \wedge q \wedge r) \vee (p \wedge q \wedge (\sim r)) \vee ((\sim p) \wedge (\sim q) \wedge (\sim r)).$$
>
>    * **For CNF (Conjunction of $\vee$-clauses)**: Look at the rows where $A = \text{F}$ (i.e., $\sim A = \text{T}$). For each $\text{F}$-row, write a disjunction $(\dots \vee \dots)$ with **flipped** literals ($\text{F} \mapsto p_j$ and $\text{T} \mapsto \sim p_j$), then join those clauses with $\wedge$!
> 2. **Recipe B — By Algebraic Manipulation (Usually Shorter!)**:
>    * **Step 1**: Eliminate $\to$ and $\leftrightarrow$ using $(X \to Y) \equiv ((\sim X) \vee Y)$ and $(X \leftrightarrow Y) \equiv ((X \wedge Y) \vee ((\sim X) \wedge (\sim Y)))$ (or $(((\sim X) \vee Y) \wedge ((\sim Y) \vee X))$).
>    * **Step 2**: Push all negations $\sim$ all the way inside to individual variables using De Morgan's Laws and $\sim(\sim X) \equiv X$.
>    * **Step 3**:
>      * For **DNF**, distribute $\wedge$ over $\vee$: $X \wedge (Y \vee Z) \equiv (X \wedge Y) \vee (X \wedge Z)$.
>      * For **CNF**, distribute $\vee$ over $\wedge$: $X \vee (Y \wedge Z) \equiv (X \vee Y) \wedge (X \vee Z)$.

> 📝 **My Study Notes (Section 1.4)**:
> * *Key takeaway to remember:*

## 1.5 Adequate Sets of Connectives

### 📘 Definition 1.10: Adequate Set of Connectives
A set of connectives is called **adequate** (or *functionally complete*) if every truth function can be represented by a statement form containing **only** connectives from that set ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Propositional Proof Systems Reference](https://www.cs.toronto.edu/~sacook/homepage/cook_reckhow.pdf)).

### 🏆 Proposition 1.8 & Corollary 1.9: Standard Adequate Sets
1. **Proposition 1.8**: The set $\{\sim, \wedge, \vee\}$ is an adequate set of connectives ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
   * *Proof*: Immediate from Proposition 1.5 (every truth function is determined by a restricted statement form). $\blacksquare$
2. **Corollary 1.9**: Each of the two-connective sets $\{\sim, \wedge\}$, $\{\sim, \vee\}$, and $\{\sim, \to\}$ is adequate ([Logic Notes Reference](https://carlosareces.github.io/ls/bibliography/ch1-lassaigne-rougemont.pdf)):
   * **Why $\{\sim, \wedge\}$ is adequate**: We can express $\vee$ using only $\sim$ and $\wedge$:

     $$(A \vee B) \equiv \sim((\sim A) \wedge (\sim B)).$$

   * **Why $\{\sim, \vee\}$ is adequate**: We can express $\wedge$ using only $\sim$ and $\vee$:

     $$(A \wedge B) \equiv \sim((\sim A) \vee (\sim B)).$$

   * **Why $\{\sim, \to\}$ is adequate**: We can express $\vee$ (and $\wedge$) using only $\sim$ and $\to$:

     $$(A \vee B) \equiv ((\sim A) \to B), \qquad (A \wedge B) \equiv \sim(A \to (\sim B)).$$

     *(This is why Chapter 2 builds the formal system $L$ using only $\sim$ and $\to$!)*

### 📘 Definition 1.11: Joint Denial ($\downarrow$, NOR) and Alternative Denial ($\mid$, NAND)
Among the 16 binary truth functions, two connectives can each generate **all** truth functions completely on their own ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)):
* **Joint Denial / NOR ($\downarrow$, Peirce's Arrow)**: $(p \downarrow q)$ means *"neither $p$ nor $q$"*, i.e., $\sim(p \vee q)$.
* **Alternative Denial / NAND ($\mid$, Sheffer's Stroke)**: $(p \mid q)$ means *"not both $p$ and $q$"*, i.e., $\sim(p \wedge q)$.

| $p$ | $q$ | $(p \downarrow q) \equiv \sim(p \vee q)$ | $(p \mid q) \equiv \sim(p \wedge q)$ | Source |
| :---: | :---: | :---: | :---: | :--- |
| $\text{T}$ | $\text{T}$ | $\text{F}$ | $\text{F}$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| $\text{T}$ | $\text{F}$ | $\text{F}$ | $\text{T}$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| $\text{F}$ | $\text{T}$ | $\text{F}$ | $\text{T}$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| $\text{F}$ | $\text{F}$ | $\text{T}$ | $\text{T}$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |

### 🏆 Proposition 1.10: Adequacy and Uniqueness of $\{\downarrow\}$ and $\{\mid\}$
> **Statement**: Each of the singleton sets $\{\downarrow\}$ and $\{\mid\}$ is an adequate set of connectives. Moreover, $\downarrow$ and $\mid$ are the **only** binary connectives which alone form an adequate set ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

* **Proof of Part 1 (Expressing $\{\sim, \vee\}$ from $\downarrow$ and $\{\sim, \wedge\}$ from $\mid$)**:
  * **Using only $\downarrow$ (NOR)**:

    $$\sim p \equiv (p \downarrow p)$$

    $$(p \vee q) \equiv \sim(p \downarrow q) \equiv ((p \downarrow q) \downarrow (p \downarrow q))$$

    $$(p \wedge q) \equiv ((\sim p) \downarrow (\sim q)) \equiv ((p \downarrow p) \downarrow (q \downarrow q))$$

    Since $\{\sim, \vee\}$ is adequate, $\{\downarrow\}$ is adequate ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).
  * **Using only $\mid$ (NAND)**:

    $$\sim p \equiv (p \mid p)$$

    $$(p \wedge q) \equiv \sim(p \mid q) \equiv ((p \mid q) \mid (p \mid q))$$

    $$(p \vee q) \equiv ((\sim p) \mid (\sim q)) \equiv ((p \mid p) \mid (q \mid q))$$

    Since $\{\sim, \wedge\}$ is adequate, $\{\mid\}$ is adequate ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8)).

* **Proof of Part 2 (Why NO other binary connective $\ast$ is adequate on its own)**:
  Suppose $\ast$ is a binary connective such that $\{\ast\}$ is adequate.
  1. **Row 1 must be $\text{F}$ ($(\text{T} \ast \text{T}) = \text{F}$)**:
     Suppose instead that $(\text{T} \ast \text{T}) = \text{T}$. Then by induction on the number of occurrences of $\ast$ in any statement form $A$ built using only $\ast$, if all variables are assigned $\text{T}$, then $A$ takes the value $\text{T}$. Thus $\sim p$ (which is $\text{F}$ when $p = \text{T}$) can never be represented—contradiction! Hence $(\text{T} \ast \text{T}) = \text{F}$.
  2. **Row 4 must be $\text{T}$ ($(\text{F} \ast \text{F}) = \text{T}$)**:
     Suppose instead that $(\text{F} \ast \text{F}) = \text{F}$. By induction on the complexity of $A$, if all variables are assigned $\text{F}$, then $A$ takes the value $\text{F}$. Thus $\sim p$ (which is $\text{T}$ when $p = \text{F}$) can never be represented—contradiction! Hence $(\text{F} \ast \text{F}) = \text{T}$.
  3. **Analyzing the 4 remaining possibilities for Rows 2 and 3 ($(\text{T} \ast \text{F})$ and $(\text{F} \ast \text{T})$)**:
     * If $(\text{T} \ast \text{F}) = \text{F}$ and $(\text{F} \ast \text{T}) = \text{F}$: the table is $(\text{F}, \text{F}, \text{F}, \text{T})$, which is **$\downarrow$ (NOR)**.
     * If $(\text{T} \ast \text{F}) = \text{T}$ and $(\text{F} \ast \text{T}) = \text{T}$: the table is $(\text{F}, \text{T}, \text{T}, \text{T})$, which is **$\mid$ (NAND)**.
     * If $(\text{T} \ast \text{F}) = \text{F}$ and $(\text{F} \ast \text{T}) = \text{T}$: the table is $(\text{F}, \text{F}, \text{T}, \text{T})$, so $(p \ast q) \equiv \sim p$. By induction, any formula built from $p, q$ using only $\ast$ is equivalent to $p, \sim p, q,$ or $\sim q$, so $(p \wedge q)$ cannot be expressed.
     * If $(\text{T} \ast \text{F}) = \text{T}$ and $(\text{F} \ast \text{T}) = \text{F}$: the table is $(\text{F}, \text{T}, \text{F}, \text{T})$, so $(p \ast q) \equiv \sim q$, which fails for the same reason.
     Therefore, $\downarrow$ and $\mid$ are the **unique** singleton adequate binary connectives! $\blacksquare$

> 🧠 **Exam Playbook — How to Prove a Connective Set $S$ is INADEQUATE**:
> Whenever an exam question asks *"Is the set $S$ adequate? Justify your answer"*, if $S$ is **not** adequate, use **structural induction on the number of connectives** in a statement form $A$ built from $S$ to show one of these invariant properties:
> 1. **$\text{T}$-Preserving Invariant** (e.g., $S = \{\wedge, \vee, \to, \leftrightarrow\}$): When all variables are $\text{T}$, every connective in $S$ outputs $\text{T}$. By induction, every formula built from $S$ has $\text{T}$ in the top row of its truth table, so $\sim p$ cannot be expressed.
> 2. **$\text{F}$-Preserving Invariant** (e.g., $S = \{\wedge, \vee\}$): When all variables are $\text{F}$, every formula built from $S$ outputs $\text{F}$, so $\sim p$ and tautologies cannot be expressed.
> 3. **Even-Parity $\text{T}$ Count Invariant** (e.g., $S = \{\sim, \leftrightarrow\}$): In a 2-variable ($4$-row) truth table, $p$ and $q$ each have $2$ (an even number of) $\text{T}$'s. Applying $\sim$ or $\leftrightarrow$ to truth columns with an even number of $\text{T}$'s ($0, 2,$ or $4$) always produces a column with an even number of $\text{T}$'s ($0, 2,$ or $4$). Thus $(p \wedge q)$ (which has $1$ $\text{T}$) and $(p \vee q)$ (which has $3$ $\text{T}$'s) can **never** be expressed using $\{\sim, \leftrightarrow\}$!
> 4. **Componentwise / Unary-Like Invariant** (e.g., $S = \{\sim\}$): Every formula in $p, q$ built from $\{\sim\}$ is equivalent to $p, \sim p, q,$ or $\sim q$.

> 📝 **My Study Notes (Section 1.5)**:
> * *Key takeaway to remember:*

## 1.6 Arguments and Validity

### 📘 Definitions 1.12 & 1.13: Argument Form and Validity
* **Definition 1.12 (Argument and Argument Form)**:
  * An **argument** is a finite sequence of statements, the last of which is the **conclusion** and the preceding ones are the **premisses** (premises) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Outline of Logic Reference](https://uodiyala.edu.iq/uploads/PDF%20ELIBRARY%20UODIYALA/EL51/Outline%20of%20Logic.pdf)).
  * An **argument form** is a finite sequence of statement forms $A_1, A_2, \dots, A_n, B$, where $A_1, \dots, A_n$ are the **premisses** and $B$ is the **conclusion** (often written $A_1, A_2, \dots, A_n \therefore B$) ([Jesse Jing's Chapter 1 Notes](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8); [Features of Propositional Logic](https://www.m-hikari.com/pms/pms-2021/pms-1-2021/p/alodhariPMS1-2021.pdf)).
* **Definition 1.13 (Valid Argument Form)**:
  An argument form $A_1, A_2, \dots, A_n \therefore B$ is **valid** if there is **no** assignment of truth values to the statement variables appearing in $A_1, \dots, A_n, B$ which gives $A_1, A_2, \dots, A_n$ all the value $\text{T}$ while giving $B$ the value $\text{F}$ ([Features of Propositional Logic](https://www.m-hikari.com/pms/pms-2021/pms-1-2021/p/alodhariPMS1-2021.pdf)). Otherwise, it is **invalid**. An argument is **valid** if its argument form is valid.

### 🏆 Proposition 1.11: Tautology Criterion for Validity
> **Statement**: An argument form with premisses $A_1, A_2, \dots, A_n$ and conclusion $B$ is **valid** if and only if the statement form
>
> $$((A_1 \wedge A_2 \wedge \dots \wedge A_n) \to B)$$
>
> is a **tautology** (i.e., $(A_1 \wedge \dots \wedge A_n)$ logically implies $B$) ([Features of Propositional Logic](https://www.m-hikari.com/pms/pms-2021/pms-1-2021/p/alodhariPMS1-2021.pdf)).

* **Proof**:
  By the truth table of $\to$, $((A_1 \wedge \dots \wedge A_n) \to B)$ fails to be a tautology if and only if there is an assignment of truth values under which $(A_1 \wedge \dots \wedge A_n)$ has value $\text{T}$ and $B$ has value $\text{F}$. By the truth table of $\wedge$, $(A_1 \wedge \dots \wedge A_n)$ has value $\text{T}$ if and only if every $A_i$ ($1 \le i \le n$) has value $\text{T}$. Therefore, $((A_1 \wedge \dots \wedge A_n) \to B)$ is a tautology if and only if no truth-value assignment makes all of $A_1, \dots, A_n$ true and $B$ false, which is the exact definition of validity ([Features of Propositional Logic](https://www.m-hikari.com/pms/pms-2021/pms-1-2021/p/alodhariPMS1-2021.pdf)). $\blacksquare$

> ⚡ **Fast Exam Technique — The Short (Indirect) Truth-Table Test for Validity**:
> Never write a 16-row or 32-row truth table to test an argument with $4$ or $5$ variables! Instead, work backward ([Sentential Logic Reference](https://euclid.colorado.edu/~monkd/logic/logic01.pdf)):
> 1. **Set the conclusion $B = \text{F}$** and **set every premise $A_1 = \text{T}, \dots, A_n = \text{T}$**.
> 2. Follow the forced variable values step-by-step:
>    * If you reach an unavoidable **contradiction** (some variable $p_i$ must be both $\text{T}$ and $\text{F}$), then no bad row exists $\implies$ the argument is **VALID**!
>    * If you find a consistent truth assignment making all $A_i = \text{T}$ and $B = \text{F}$, state that assignment explicitly $\implies$ the argument is **INVALID**!

> 📝 **My Study Notes (Section 1.6)**:
> * *Key takeaway to remember:*

# PART II — Chapter 2: Formal Statement Calculus

## 2.1 The Formal System $L$

### 🌟 Why Formalize? Syntax vs. Semantics
In Chapter 1, our notions of *tautology* and *validity* relied on **meaning (semantics)**—specifically, assigning truth values $\text{T}$ and $\text{F}$. In Chapter 2, we build a purely **syntactic** axiomatic system $L$ where proofs are finite sequences of strings manipulated by explicit rules without ever mentioning "true" or "false" ([Propositional Formula Overview](https://en.wikipedia.org/wiki/Propositional_formula); [Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).

### 📘 Definition 2.1: Alphabet, Well-Formed Formulas (wfs.), Axioms, and Rule of $L$
A formal system has four components: an **alphabet**, a set of **well-formed formulas (wfs.)**, a set of **axioms**, and a finite set of **rules of inference** ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf); [ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf)):

1. **Alphabet of $L$**:
   * **Statement letters**: $p_1, p_2, p_3, \dots$ (countably infinite list; often written $p, q, r, \dots$).
   * **Primitive connectives**: $\sim$ and $\to$ (chosen because $\{\sim, \to\}$ is adequate by Corollary 1.9!).
   * **Punctuation**: $($ and $)$.

2. **Well-Formed Formulas (wfs.) of $L$ (Inductive Definition)**:
   * **Rule 1**: Every statement letter $p_i$ is a wf. of $L$.
   * **Rule 2**: If $A$ and $B$ are wfs. of $L$, then $(\sim A)$ and $(A \to B)$ are wfs. of $L$.
   * **Rule 3**: Nothing else is a wf. of $L$ unless obtained by a finite number of applications of Rules 1 and 2 ([Propositional Calculus Reference](https://educational-sciences-journal.lacts.org.ly/esj/index.php/esj/article/download/216/190/372)).
   * *Abbreviations for $\vee, \wedge, \leftrightarrow$ in $L$*: For any wfs. $A, B$ of $L$:
     * $(A \vee B)$ is an abbreviation for $((\sim A) \to B)$.
     * $(A \wedge B)$ is an abbreviation for $(\sim(A \to (\sim B)))$.
     * $(A \leftrightarrow B)$ is an abbreviation for $((A \to B) \wedge (B \to A))$, i.e., $(\sim((A \to B) \to (\sim(B \to A))))$.

3. **Axioms of $L$ (Łukasiewicz's Three Axiom Schemes)**:
   For **any** wfs. $A, B, C$ of $L$, the following are axioms of $L$ ([Math StackExchange Hamilton Section 2.1 Reference](https://math.stackexchange.com/questions/3482453/how-to-prove-a-to-b-to-a-to-a-using-lukasiewiczs-axioms-mp-and-deduct); [ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf)):
   * **(L1)**: $(A \to (B \to A))$
   * **(L2)**: $((A \to (B \to C)) \to ((A \to B) \to (A \to C)))$
   * **(L3)**: $(((\sim A) \to (\sim B)) \to (B \to A))$
   > 💡 **Note**: (L1), (L2), and (L3) are **axiom schemes**—because $A, B, C$ can be *any* wfs. of $L$, $L$ has infinitely many axioms generated by these three patterns ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)). Notice that because $A, B, C$ can be any wfs., $L$ does not need a separate rule of uniform substitution!

4. **Rule of Inference of $L$**:
   * **Modus Ponens (MP)**: From $A$ and $(A \to B)$, we may infer $B$ (we say $B$ is an *immediate consequence* of $A$ and $(A \to B)$) ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).

### 📘 Definitions 2.2 & 2.3: Proof, Theorem ($\vdash_L A$), and Deduction from Hypotheses ($\Gamma \vdash_L A$)

* **Definition 2.2 (Proof and Theorem in $L$)**:
  A **proof in $L$** is a finite sequence $A_1, A_2, \dots, A_m$ of wfs. of $L$ such that for each $i$ ($1 \le i \le m$), either:
  1. $A_i$ is an **axiom** of $L$ (an instance of (L1), (L2), or (L3)), or
  2. $A_i$ is an immediate consequence of two earlier wfs. $A_j$ and $A_k = (A_j \to A_i)$ ($j, k < i$) in the sequence by **Modus Ponens (MP)**.
  A wf. $A$ is a **theorem of $L$**, written $\vdash_L A$ (or $\vdash A$), if there exists a proof $A_1, \dots, A_m$ in $L$ with last formula $A_m = A$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).

* **Definition 2.3 (Deduction / Consequence from a Set of Hypotheses $\Gamma$)**:
  Let $\Gamma$ be a set of wfs. of $L$. A **deduction of $A$ from $\Gamma$ in $L$** is a finite sequence $A_1, A_2, \dots, A_m$ of wfs. of $L$ such that $A_m = A$ and for each $i$ ($1 \le i \le m$), either:
  1. $A_i$ is an **axiom** of $L$, or
  2. $A_i \in \Gamma$ ($A_i$ is a **premise / hypothesis** from $\Gamma$), or
  3. $A_i$ is inferred from two earlier wfs. $A_j$ and $A_k = (A_j \to A_i)$ ($j, k < i$) by **MP**.
  If such a deduction exists, we say $A$ is a **consequence of $\Gamma$ in $L$** (or *deducible from $\Gamma$*), and write $\Gamma \vdash_L A$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).

* **Basic Properties of $\Gamma \vdash_L A$ (Always Keep in Mind for Proofs)**:
  1. $\emptyset \vdash_L A$ is the exact same thing as $\vdash_L A$.
  2. If $A \in \Gamma$, then $\Gamma \vdash_L A$ (via the 1-line deduction $A$).
  3. **Monotonicity**: If $\Gamma \subseteq \Delta$ and $\Gamma \vdash_L A$, then $\Delta \vdash_L A$.
  4. **Finiteness Property**: Since any deduction $A_1, \dots, A_m$ is finite, if $\Gamma \vdash_L A$, then there is a **finite subset** $\Gamma_0 \subseteq \Gamma$ such that $\Gamma_0 \vdash_L A$.

### 🏆 Lemma 2.1 (Identity Theorem): $\vdash_L (A \to A)$
> **Statement**: For any wf. $A$ of $L$, $\vdash_L (A \to A)$ ([ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf); [Introduction to Mathematical Logic Reference](https://www.researchgate.net/publication/300051677_Introduction_to_Mathematical_Logic)).

> ⚠️ **Exam Essential**: This 5-line proof is one of the most frequently tested proofs in mathematical logic because it is required *inside* Case 2 of the proof of the Deduction Theorem (so you **cannot** use the Deduction Theorem to prove $\vdash_L (A \to A)$!).

| Step | Formula (wf. of $L$) | Justification | How to Remember It |
| :---: | :--- | :--- | :--- |
| **1** | $(A \to ((A \to A) \to A)) \to ((A \to (A \to A)) \to (A \to A))$ | Axiom **(L2)** with $B = (A \to A)$, $C = A$ | Set $B = (A \to A)$ and $C = A$ in (L2) so the end becomes $(A \to A)$ |
| **2** | $A \to ((A \to A) \to A)$ | Axiom **(L1)** with $B = (A \to A)$ | Matches the antecedent of Step 1 |
| **3** | $(A \to (A \to A)) \to (A \to A)$ | **MP** on Steps 1 and 2 | Peel off the antecedent of Step 1 |
| **4** | $A \to (A \to A)$ | Axiom **(L1)** with $B = A$ | Matches the antecedent of Step 3 |
| **5** | $A \to A$ | **MP** on Steps 3 and 4 | Peel off the antecedent of Step 3 — Done! $\blacksquare$ |

### 🏆 Proposition 2.2: The Deduction Theorem for $L$
> **Statement**: Let $\Gamma$ be a set of wfs. of $L$ and let $A, B$ be any wfs. of $L$. If $\Gamma \cup \{A\} \vdash_L B$, then $\Gamma \vdash_L (A \to B)$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf); [ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf)).
> *(More generally, $\Gamma \cup \{A\} \vdash_L B \iff \Gamma \vdash_L (A \to B)$, since the converse $\Gamma \vdash_L (A \to B) \implies \Gamma \cup \{A\} \vdash_L B$ follows immediately by adding $A$ and applying MP.)*

* **Complete Proof (by Induction on the Length $m$ of the Deduction of $B$ from $\Gamma \cup \{A\}$)**:
  Let $B_1, B_2, \dots, B_m$ be a deduction of $B$ from $\Gamma \cup \{A\}$ (so $B_m = B$). We prove by strong induction on $i$ ($1 \le i \le m$) that $\Gamma \vdash_L (A \to B_i)$ for every $i$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)):
  * **Case 1 ($B_i$ is an axiom of $L$ or $B_i \in \Gamma$)**:
    We can deduce $(A \to B_i)$ from $\Gamma$ in 3 lines:
    1. $B_i$ — Axiom of $L$ or member of $\Gamma$.
    2. $B_i \to (A \to B_i)$ — Instance of Axiom **(L1)**.
    3. $A \to B_i$ — From (1) and (2) by **MP**.
    Thus $\Gamma \vdash_L (A \to B_i)$.
  * **Case 2 ($B_i$ is $A$)**:
    By Lemma 2.1, $\vdash_L (A \to A)$, and therefore $\Gamma \vdash_L (A \to B_i)$.
  * **Case 3 ($B_i$ is inferred by MP from $B_j$ and $B_k = (B_j \to B_i)$ with $j, k < i$)**:
    By the induction hypothesis applied to $j < i$ and $k < i$, we already have deductions showing $\Gamma \vdash_L (A \to B_j)$ and $\Gamma \vdash_L (A \to (B_j \to B_i))$. We append to those deductions:
    1. $A \to B_j$ — Induction hypothesis ($j < i$).
    2. $A \to (B_j \to B_i)$ — Induction hypothesis ($k < i$).
    3. $(A \to (B_j \to B_i)) \to ((A \to B_j) \to (A \to B_i))$ — Instance of Axiom **(L2)**.
    4. $(A \to B_j) \to (A \to B_i)$ — From (2) and (3) by **MP**.
    5. $A \to B_i$ — From (1) and (4) by **MP**.
    Thus $\Gamma \vdash_L (A \to B_i)$.
  Setting $i = m$ gives $\Gamma \vdash_L (A \to B)$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)). $\blacksquare$

> 💡 **Why the Deduction Theorem is Your Best Friend on the Midterm**:
> Trying to prove $\vdash_L (A \to (B \to C))$ directly from (L1)–(L3) can take 15+ lines of painful (L2) nested brackets. With the **Deduction Theorem (DT)**, to prove $\vdash_L (A \to (B \to C))$, it suffices to show $\{A, B\} \vdash_L C$, and then apply DT twice!

### 🏆 Essential Derived Lemmas in $L$ (Section 2.1 Toolkit)
In Hamilton's Section 2.1, the following core lemmas are proved using (L1)–(L3), MP, and the Deduction Theorem (DT) ([ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf); [Equivalence of Axiom Sets in L](https://vixra.org/pdf/2504.0181v1.pdf)). Memorize both their statements and their short DT proofs!

#### 1. Lemma 2.3 (Hypothetical Syllogism / Transitivity of $\to$)
* **Statements**:

  $$\{A \to B, \ B \to C\} \vdash_L (A \to C) \qquad \text{and} \qquad \vdash_L ((A \to B) \to ((B \to C) \to (A \to C)))$$

* **Proof**:
  1. $A \to B$ (Hypothesis)
  2. $B \to C$ (Hypothesis)
  3. $A$ (Hypothesis)
  4. $B$ (MP on 1, 3)
  5. $C$ (MP on 2, 4)
  Thus $\{A \to B, B \to C, A\} \vdash_L C$. Applying the Deduction Theorem once gives $\{A \to B, B \to C\} \vdash_L (A \to C)$, and applying DT two more times yields $\vdash_L ((A \to B) \to ((B \to C) \to (A \to C)))$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)). $\blacksquare$

#### 2. Lemma 2.4 (Ex Falso Quodlibet / Duns Scotus Law)
* **Statements**:

  $$\vdash_L ((\sim A) \to (A \to B)) \qquad \text{and} \qquad \{\sim A, \ A\} \vdash_L B$$

  *(From a contradiction $\sim A$ and $A$, any formula $B$ can be deduced in $L$!)*
* **Proof**:
  1. $\sim A$ (Hypothesis)
  2. $A$ (Hypothesis)
  3. $(\sim A) \to ((\sim B) \to (\sim A))$ (Axiom **(L1)**)
  4. $(\sim B) \to (\sim A)$ (MP on 1, 3)
  5. $((\sim B) \to (\sim A)) \to (A \to B)$ (Axiom **(L3)**)
  6. $A \to B$ (MP on 4, 5)
  7. $B$ (MP on 2, 6)
  Thus $\{\sim A, A\} \vdash_L B$. Applying DT twice gives $\vdash_L ((\sim A) \to (A \to B))$ ([ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf)). $\blacksquare$

#### 3. Lemma 2.5 (Double Negation Elimination & Introduction)
* **Part (a) — Double Negation Elimination**:

  $$\vdash_L ((\sim(\sim A)) \to A)$$

  * **Proof**:
    1. $(\sim(\sim A)) \to ((\sim A) \to (\sim(\sim(\sim A))))$ (Lemma 2.4 with $\sim A$ for $A$ and $\sim(\sim(\sim A))$ for $B$)
    2. $(((\sim A) \to (\sim(\sim(\sim A)))) \to ((\sim(\sim A)) \to A))$ (Axiom **(L3)**)
    3. $(\sim(\sim A)) \to ((\sim(\sim A)) \to A)$ (Syllogism Lemma 2.3 on 1, 2)
    4. $((\sim(\sim A)) \to ((\sim(\sim A)) \to A)) \to (((\sim(\sim A)) \to (\sim(\sim A))) \to ((\sim(\sim A)) \to A))$ (Axiom **(L2)**)
    5. $((\sim(\sim A)) \to (\sim(\sim A))) \to ((\sim(\sim A)) \to A)$ (MP on 3, 4)
    6. $(\sim(\sim A)) \to (\sim(\sim A))$ (Lemma 2.1)
    7. $(\sim(\sim A)) \to A$ (MP on 5, 6) ([ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf)). $\blacksquare$
* **Part (b) — Double Negation Introduction**:

  $$\vdash_L (A \to (\sim(\sim A)))$$

  * **Proof**:
    1. $(\sim(\sim(\sim A))) \to (\sim A)$ (Instance of Lemma 2.5(a) with $\sim A$ in place of $A$)
    2. $((\sim(\sim(\sim A))) \to (\sim A)) \to (A \to (\sim(\sim A)))$ (Axiom **(L3)**)
    3. $A \to (\sim(\sim A))$ (MP on 1, 2) ([ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf)). $\blacksquare$

#### 4. Lemma 2.6 (Contraposition Laws in $L$)
* **Statements**:
  1. $\vdash_L ((A \to B) \to ((\sim B) \to (\sim A)))$
  2. $\vdash_L ((A \to (\sim B)) \to (B \to (\sim A)))$
  3. $\vdash_L (((\sim A) \to B) \to ((\sim B) \to A))$
* **Proof of (1)**:
  Assume $A \to B$ and $\sim(\sim A)$. Since $\vdash_L ((\sim(\sim A)) \to A)$ (Lemma 2.5(a)), MP gives $A$, then MP with $A \to B$ gives $B$, and Lemma 2.5(b) $\vdash_L (B \to (\sim(\sim B)))$ gives $\sim(\sim B)$. By the Deduction Theorem, $\{A \to B\} \vdash_L ((\sim(\sim A)) \to (\sim(\sim B)))$. Applying Axiom **(L3)** $(((\sim(\sim A)) \to (\sim(\sim B))) \to ((\sim B) \to (\sim A)))$ and MP gives $\{A \to B\} \vdash_L ((\sim B) \to (\sim A))$, and DT yields $\vdash_L ((A \to B) \to ((\sim B) \to (\sim A)))$ ([ViXra Formal System L Reference](https://vixra.org/pdf/1904.0095v1.pdf)). $\blacksquare$

#### 5. Lemma 2.7 (Reductio ad Absurdum & Proof by Cases in $L$)
* **Statements**:
  1. $\vdash_L (((\sim A) \to B) \to (((\sim A) \to (\sim B)) \to A))$
  2. **Meta-Rule of Reductio ad Absurdum**: If $\Gamma \cup \{\sim A\} \vdash_L B$ and $\Gamma \cup \{\sim A\} \vdash_L (\sim B)$, then $\Gamma \vdash_L A$ ([Equivalence of Axiom Sets in L](https://vixra.org/pdf/2504.0181v1.pdf)).
  3. **Proof by Cases Lemma**: $\vdash_L ((A \to B) \to (((\sim A) \to B) \to B))$ (so if $\Gamma \cup \{A\} \vdash_L B$ and $\Gamma \cup \{\sim A\} \vdash_L B$, then $\Gamma \vdash_L B$).
* **Proof of (1) & (2)**:
  Assume $(\sim A) \to B$, $(\sim A) \to (\sim B)$, and $\sim A$. By MP twice we obtain both $B$ and $\sim B$. From $\{\sim B, B\}$ we can deduce $\sim(\sim A)$ by Lemma 2.4 ($\{\sim B, B\} \vdash_L \sim(\sim A)$). By the Deduction Theorem, $\{(\sim A) \to B, (\sim A) \to (\sim B)\} \vdash_L ((\sim A) \to (\sim(\sim A)))$. Also by Lemma 2.1, $\vdash_L ((\sim A) \to (\sim A))$. Applying Axiom **(L3)** $(((\sim A) \to (\sim(\sim A))) \to ((\sim A) \to A))$ gives $(\sim A) \to A$, and combining $\sim A \to A$ with $(\sim A) \to (A \to \sim((\sim A) \to A))$ via **(L2)** and **(L3)** yields $A$! Applying DT twice proves (1), and (2) follows immediately from (1) and DT ([Equivalence of Axiom Sets in L](https://vixra.org/pdf/2504.0181v1.pdf)). $\blacksquare$
* **Proof of (3)**:
  Assume $A \to B$ and $(\sim A) \to B$. By Contraposition (Lemma 2.6(1)), $A \to B$ gives $(\sim B) \to (\sim A)$, and $(\sim A) \to B$ gives $(\sim B) \to (\sim(\sim A))$. By Reductio (1) with $\sim B$ in place of $\sim A$, these two give $B$! Applying DT twice gives $\vdash_L ((A \to B) \to (((\sim A) \to B) \to B))$. $\blacksquare$

#### 6. Lemma 2.8 (Truth-Table Simulation Lemma — Used in Kalmár's Completeness Proof!)
For any wfs. $A, B$ of $L$:
1. $\{(\sim A)\} \vdash_L (A \to B)$ — Immediate from Lemma 2.4 ($\vdash_L ((\sim A) \to (A \to B))$) and MP.
2. $\{B\} \vdash_L (A \to B)$ — Immediate from Axiom **(L1)** ($\vdash_L (B \to (A \to B))$) and MP.
3. $\{A, (\sim B)\} \vdash_L \sim(A \to B)$ —
   * *Proof*: Since $\{A \to B, A\} \vdash_L B$ by MP, the Deduction Theorem gives $\{A\} \vdash_L ((A \to B) \to B)$. Applying Contraposition Lemma 2.6(1) gives $\{A\} \vdash_L ((\sim B) \to \sim(A \to B))$, and MP with $\sim B$ gives $\{A, (\sim B)\} \vdash_L \sim(A \to B)$! $\blacksquare$

> 📝 **My Study Notes (Section 2.1)**:
> * *Key takeaway to remember:*

## 2.2 The Adequacy Theorem for $L$ (Soundness, Consistency & Completeness)

### 📘 Definition 2.4: Valuation $v$ of $L$ and Tautology in $L$
To connect the formal system $L$ back to truth tables, Hamilton defines a **valuation** directly on the wfs. of $L$ ([Propositional Formula Overview](https://en.wikipedia.org/wiki/Propositional_formula); [Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)):

* **Definition 2.4 (Valuation of $L$, p. 37)**:
  A **valuation of $L$** is a function $v$ from the set of all wfs. of $L$ into the set $\{\text{T}, \text{F}\}$ such that for all wfs. $A, B$ of $L$:
  1. $v((\sim A)) \ne v(A)$ (i.e., $v((\sim A)) = \text{T} \iff v(A) = \text{F}$).
  2. $v((A \to B)) = \text{F} \iff (v(A) = \text{T} \text{ and } v(B) = \text{F})$.
* **Proposition 2.9 (Uniqueness & Extension of Valuations)**:
  Every assignment of truth values $\{\text{T}, \text{F}\}$ to the statement letters $p_1, p_2, \dots$ extends to a **unique** valuation $v$ on the set of all wfs. of $L$; furthermore, $v(A)$ depends only on the values assigned by $v$ to the statement letters that actually occur in $A$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).
* **Definition 2.5 (Tautology in $L$, $\models_L A$)**:
  A wf. $A$ of $L$ is a **tautology of $L$** (written $\models_L A$) if $v(A) = \text{T}$ for **every** valuation $v$ of $L$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).

### 🏆 Proposition 2.10: The Soundness Theorem for $L$
> **Statement**: Every theorem of $L$ is a tautology:
>
> $$\text{If } \vdash_L A, \text{ then } \models_L A.$$
>
> *(More generally, if $\Gamma \vdash_L A$, then every valuation $v$ making all wfs. in $\Gamma$ true also makes $A$ true, written $\Gamma \models_L A$.)* ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf))

* **Proof (by Induction on the Length of a Proof in $L$)**:
  Let $A_1, A_2, \dots, A_m$ be a proof of $A$ in $L$ (so $A_m = A$). We verify by induction on $i$ ($1 \le i \le m$) that $A_i$ is a tautology:
  1. **Base / Axiom Step**: It is easy to check by 2-variable or 3-variable truth tables (or directly from Definition 2.4) that every instance of Axiom Schemes **(L1)**, **(L2)**, and **(L3)** takes the value $\text{T}$ under every valuation $v$.
  2. **Inductive / MP Step**: Suppose $A_i$ is inferred by MP from $A_j$ and $A_k = (A_j \to A_i)$ ($j, k < i$). By the induction hypothesis, $A_j$ and $(A_j \to A_i)$ are tautologies. By Proposition 1.1 (or Definition 2.4(2): if $v(A_j) = \text{T}$ and $v(A_j \to A_i) = \text{T}$, we cannot have $v(A_i) = \text{F}$), $v(A_i) = \text{T}$ for every valuation $v$. Thus $A_i$ is a tautology.
  In particular, $A_m = A$ is a tautology ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)). $\blacksquare$

### 🏆 Corollary 2.11: Consistency of $L$
> **Statement**: $L$ is **consistent**; that is, there is no wf. $A$ of $L$ such that both $\vdash_L A$ and $\vdash_L (\sim A)$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).

* **Proof**:
  Suppose for contradiction that there is a wf. $A$ such that $\vdash_L A$ and $\vdash_L (\sim A)$. By the Soundness Theorem (Proposition 2.10), both $A$ and $(\sim A)$ would be tautologies, meaning $v(A) = \text{T}$ and $v((\sim A)) = \text{T}$ for every valuation $v$, contradicting $v((\sim A)) \ne v(A)$ (Definition 2.4(1)). $\blacksquare$

### 🏆 The Adequacy (Completeness) Theorem for $L$: Two Classic Proofs!
The **Adequacy Theorem for $L$** states the converse of Soundness:

$$\text{Every wf. } A \text{ of } L \text{ which is a tautology is a theorem of } L \quad (\models_L A \implies \vdash_L A).$$

Hamilton presents both the **direct constructive proof (via Kalmár's Lemma)** and the **extensions / Lindenbaum's Lemma method** (which generalizes to First-Order Predicate Calculus in Chapter 4!) ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l); [Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)). Make sure you know **both** for your midterm!

#### Method 1 — Direct Proof via Kalmár's Evaluation Lemma

* **Lemma 2.12 (Kalmár's Lemma)**:
  Let $A$ be a wf. of $L$ whose statement letters are among $p_1, p_2, \dots, p_n$, and let $v$ be any valuation of $L$. For each $i \in \{1, \dots, n\}$, define:

  $$p_i' = \begin{cases} p_i & \text{if } v(p_i) = \text{T}, \\ (\sim p_i) & \text{if } v(p_i) = \text{F}, \end{cases} \qquad \text{and} \qquad A' = \begin{cases} A & \text{if } v(A) = \text{T}, \\ (\sim A) & \text{if } v(A) = \text{F}. \end{cases}$$

  Then:

  $$\{p_1', p_2', \dots, p_n'\} \vdash_L A'.$$

  * **Proof (by Strong Induction on the Number $k$ of Connectives in $A$)**:
    * **Base Case ($k = 0$)**: $A$ is a statement letter $p_i$ for some $1 \le i \le n$. Then $A' = p_i'$, and clearly $\{p_1', \dots, p_n'\} \vdash_L p_i'$.
    * **Inductive Step ($k > 0$)**: Suppose the lemma holds for all wfs. with fewer than $k$ connectives.
      * **Subcase 1 ($A$ is $(\sim B)$)**: $B$ has $k - 1$ connectives.
        * If $v(B) = \text{T}$, then $v(A) = \text{F}$, so $B' = B$ and $A' = (\sim A) = (\sim(\sim B))$. By the induction hypothesis, $\{p_1', \dots, p_n'\} \vdash_L B$. Since $\vdash_L (B \to (\sim(\sim B)))$ (Lemma 2.5(b)), MP gives $\{p_1', \dots, p_n'\} \vdash_L (\sim(\sim B))$, i.e., $A'$.
        * If $v(B) = \text{F}$, then $v(A) = \text{T}$, so $B' = (\sim B) = A = A'$. By the induction hypothesis, $\{p_1', \dots, p_n'\} \vdash_L (\sim B)$, which is literally $\{p_1', \dots, p_n'\} \vdash_L A'$.
      * **Subcase 2 ($A$ is $(B \to C)$)**: $B$ and $C$ each have fewer than $k$ connectives, so by the induction hypothesis $\{p_1', \dots, p_n'\} \vdash_L B'$ and $\{p_1', \dots, p_n'\} \vdash_L C'$. There are three sub-possibilities for $v(B)$ and $v(C)$:
        1. $v(B) = \text{F}$: Then $v(A) = \text{T}$, so $B' = (\sim B)$ and $A' = (B \to C)$. From $\{p_1', \dots, p_n'\} \vdash_L (\sim B)$ and Lemma 2.8(1) ($\{(\sim B)\} \vdash_L (B \to C)$), we get $\{p_1', \dots, p_n'\} \vdash_L A'$.
        2. $v(C) = \text{T}$: Then $v(A) = \text{T}$, so $C' = C$ and $A' = (B \to C)$. From $\{p_1', \dots, p_n'\} \vdash_L C$ and Lemma 2.8(2) ($\{C\} \vdash_L (B \to C)$), we get $\{p_1', \dots, p_n'\} \vdash_L A'$.
        3. $v(B) = \text{T}$ and $v(C) = \text{F}$: Then $v(A) = \text{F}$, so $B' = B$, $C' = (\sim C)$, and $A' = \sim(B \to C)$. From $\{p_1', \dots, p_n'\} \vdash_L B$, $\{p_1', \dots, p_n'\} \vdash_L (\sim C)$, and Lemma 2.8(3) ($\{B, (\sim C)\} \vdash_L \sim(B \to C)$), we get $\{p_1', \dots, p_n'\} \vdash_L A'$. $\blacksquare$

* **Proposition 2.13 (Adequacy Theorem for $L$ — Proof from Kalmár's Lemma)**:
  Let $A$ be a tautology of $L$ with statement letters $p_1, \dots, p_n$. Since $\models_L A$, $v(A) = \text{T}$ for **every** valuation $v$, so $A'$ is **always $A$**!
  * By Lemma 2.12, for every one of the $2^n$ choices of $p_1', \dots, p_n'$, we have $\{p_1', \dots, p_{n-1}', p_n\} \vdash_L A$ (taking $v(p_n) = \text{T}$) and $\{p_1', \dots, p_{n-1}', \sim p_n\} \vdash_L A$ (taking $v(p_n) = \text{F}$).
  * By the Deduction Theorem, $\{p_1', \dots, p_{n-1}'\} \vdash_L (p_n \to A)$ and $\{p_1', \dots, p_{n-1}'\} \vdash_L ((\sim p_n) \to A)$.
  * Applying Proof by Cases (Lemma 2.7(3): $\vdash_L ((p_n \to A) \to (((\sim p_n) \to A) \to A))$) and MP twice eliminates $p_n'$, giving:

    $$\{p_1', \dots, p_{n-1}'\} \vdash_L A.$$

  * Repeating this exact elimination step $n - 1$ more times for $p_{n-1}, p_{n-2}, \dots, p_1$ eliminates all hypotheses and leaves $\emptyset \vdash_L A$, i.e., $\vdash_L A$! $\blacksquare$

#### Method 2 — Extensions, Consistency, Completeness & Lindenbaum's Lemma (Hamilton pp. 40–42)

* **📘 Definitions 2.6–2.8 (Extensions, Consistency, and Syntactic Completeness)**:
  1. **Definition 2.6 (Extension $L^*$ of $L$)**: An **extension $L^*$ of $L$** is a formal system with the exact same alphabet, wfs., and rule of inference (MP) as $L$, whose set of axioms includes all the axioms of $L$ (plus possibly additional wfs. of $L$ adjoined as new axioms) ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l)).
     * *Note*: Since $L^*$ contains all axioms of $L$ and uses only MP, the **Deduction Theorem** and all theorems/lemmas of $L$ automatically hold in $L^*$!
  2. **Definition 2.7 (Consistent Extension)**: An extension $L^*$ of $L$ is **consistent** if there is **no** wf. $A$ of $L$ such that both $\vdash_{L^*} A$ and $\vdash_{L^*} (\sim A)$. Otherwise, $L^*$ is **inconsistent** ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l)).
  3. **Definition 2.8 (Syntactically Complete Extension)**: An extension $L^*$ of $L$ is **complete** if for **every** wf. $A$ of $L$, either $\vdash_{L^*} A$ or $\vdash_{L^*} (\sim A)$ ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l); [Math StackExchange Hamilton p. 41 Completeness Quote](https://math.stackexchange.com/questions/5140939/can-propositional-logic-be-made-incomplete-under-an-interpretation-just-like-g%C3%B6d)).
     > ⚠️ **Exam Watchout (Two Meanings of "Complete"!)**:
     > * **Semantic Completeness (Adequacy of $L$)**: Every *tautology* is a theorem of $L$ ($\models_L A \implies \vdash_L A$). $L$ **DOES** have this property!
     > * **Syntactic Completeness (Definition 2.8)**: For *every* wf. $A$, either $\vdash A$ or $\vdash (\sim A)$. As Hamilton remarks on p. 41, **$L$ itself is NOT syntactically complete**—for instance, for the statement letter $p_1$, neither $\vdash_L p_1$ nor $\vdash_L (\sim p_1)$ because neither $p_1$ nor $(\sim p_1)$ is a tautology ([Math StackExchange Hamilton p. 41 Completeness Quote](https://math.stackexchange.com/questions/5140939/can-propositional-logic-be-made-incomplete-under-an-interpretation-just-like-g%C3%B6d))!

* **🏆 Lemma 2.14 (Inconsistency Explosion)**:
  > **Statement**: An extension $L^*$ of $L$ is inconsistent if and only if **every** wf. $B$ of $L$ is a theorem of $L^*$ ($\vdash_{L^*} B$ for all $B$).
  * **Proof**:
    * $(\Rightarrow)$ If $L^*$ is inconsistent, there is some wf. $A$ such that $\vdash_{L^*} A$ and $\vdash_{L^*} (\sim A)$. Since $\vdash_L ((\sim A) \to (A \to B))$ (Lemma 2.4) holds in $L^*$, applying MP twice yields $\vdash_{L^*} B$ for any wf. $B$.
    * $(\Leftarrow)$ If every wf. is a theorem of $L^*$, then for any $A$, both $\vdash_{L^*} A$ and $\vdash_{L^*} (\sim A)$, so $L^*$ is inconsistent. $\blacksquare$

* **🏆 Proposition 2.15 (Adjoining the Negation of a Non-Theorem Preserves Consistency, p. 41)**:
  > **Statement**: Let $L^*$ be a consistent extension of $L$, and let $A$ be a wf. of $L$ which is **not** a theorem of $L^*$ ($\not\vdash_{L^*} A$). Let $L^{**}$ be the extension of $L^*$ obtained by adding $(\sim A)$ as a new axiom. Then $L^{**}$ is also **consistent** ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l)).
  * **Proof**:
    Suppose for contradiction that $L^{**}$ is **inconsistent**. Then there is some wf. $B$ such that $\vdash_{L^{**}} B$ and $\vdash_{L^{**}} (\sim B)$. Since the axioms of $L^{**}$ are just the axioms of $L^*$ together with $(\sim A)$, any proof in $L^{**}$ is a deduction from $\{\sim A\}$ in $L^*$. Thus:

    $$\{\sim A\} \vdash_{L^*} B \qquad \text{and} \qquad \{\sim A\} \vdash_{L^*} (\sim B).$$

    By the Deduction Theorem in $L^*$, we obtain:

    $$\vdash_{L^*} ((\sim A) \to B) \qquad \text{and} \qquad \vdash_{L^*} ((\sim A) \to (\sim B)).$$

    By Reductio ad Absurdum (Lemma 2.7(1): $\vdash_L (((\sim A) \to B) \to (((\sim A) \to (\sim B)) \to A))$) and MP twice in $L^*$, we deduce $\vdash_{L^*} A$. This contradicts the assumption that $A$ is not a theorem of $L^*$. Therefore, $L^{**}$ must be consistent ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l))! $\blacksquare$
    *(Note: Dual version—if $\not\vdash_{L^*} (\sim A)$, then adding $A$ as an axiom to $L^*$ also preserves consistency, since adding $\sim(\sim A)$ preserves consistency and $\vdash_L ((\sim(\sim A)) \leftrightarrow A)$.)*

* **🏆 Proposition 2.16 (Lindenbaum's Lemma, Hamilton p. 41)**:
  > **Statement**: Let $L^*$ be a consistent extension of $L$. Then there exists a **consistent, complete** extension $J$ of $L^*$ ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l)).
  * **Complete Proof (Hamilton p. 41)**:
    1. **Enumerate the wfs. of $L$**: Since the alphabet of $L$ is countable and each wf. is a finite string of symbols, the set of all wfs. of $L$ is countably infinite. List all wfs. of $L$ as a sequence:

       $$A_0, A_1, A_2, \dots, A_n, \dots$$

    2. **Inductive Construction of the Chain $J_0, J_1, J_2, \dots$**:
       Define a sequence of extensions $J_0, J_1, J_2, \dots$ of $L^*$ inductively:
       * Set $J_0 = L^*$.
       * For $n \ge 1$, having defined $J_{n-1}$:
         * If $\vdash_{J_{n-1}} A_{n-1}$, set $J_n = J_{n-1}$.
         * If $\not\vdash_{J_{n-1}} A_{n-1}$, let $J_n$ be the extension of $J_{n-1}$ obtained by adding $(\sim A_{n-1})$ as a new axiom.
       By Proposition 2.15 and induction on $n$, **every $J_n$ ($n \ge 0$) is consistent** ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l)).
    3. **Define $J$ as the Union of the Chain**:
       Let $J$ be the extension of $L^*$ whose set of axioms is the union of the axiom sets of $J_0, J_1, J_2, \dots$.
    4. **Prove $J$ is Consistent**:
       Suppose for contradiction that $J$ is inconsistent, so $\vdash_J B$ and $\vdash_J (\sim B)$ for some wf. $B$. Because proofs in $J$ are **finite sequences** of wfs., the proofs of $B$ and $(\sim B)$ in $J$ involve only finitely many axioms of $J$. Since the axiom sets of $J_0 \subseteq J_1 \subseteq J_2 \subseteq \dots$ form a nested chain, there exists a sufficiently large integer $N$ such that all axioms used in both proofs belong to $J_N$. Hence $\vdash_{J_N} B$ and $\vdash_{J_N} (\sim B)$, contradicting the consistency of $J_N$! Thus $J$ is consistent ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l)).
    5. **Prove $J$ is Complete**:
       Let $A$ be any wf. of $L$. Then $A = A_k$ for some index $k \ge 0$ in our enumeration.
       * If $\vdash_{J_k} A_k$, then since $J$ is an extension of $J_k$, $\vdash_J A_k$.
       * If $\not\vdash_{J_k} A_k$, then by construction $(\sim A_k)$ is an axiom of $J_{k+1}$, so $\vdash_{J_{k+1}} (\sim A_k)$, which implies $\vdash_J (\sim A_k)$.
       In either case, $\vdash_J A$ or $\vdash_J (\sim A)$. Therefore, $J$ is consistent and complete ([Math StackExchange Hamilton p. 41 Lindenbaum Proof](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l))! $\blacksquare$

* **🏆 Second Proof of the Adequacy Theorem for $L$ (Using Lindenbaum's Lemma, pp. 41–42)**:
  Suppose $A$ is a wf. of $L$ which is **not** a theorem of $L$ ($\not\vdash_L A$). We will construct a valuation $v$ of $L$ such that $v(A) = \text{F}$ (proving $A$ is not a tautology) ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)):
  1. Since $L$ is consistent (Corollary 2.11) and $\not\vdash_L A$, by Proposition 2.15 the extension $L^*$ obtained by adding $(\sim A)$ as an axiom to $L$ is **consistent**.
  2. By **Lindenbaum's Lemma (Proposition 2.16)**, $L^*$ has a **consistent and complete** extension $J$.
  3. Define a function $v$ from the set of all wfs. of $L$ into $\{\text{T}, \text{F}\}$ by:

     $$v(B) = \begin{cases} \text{T} & \text{if } \vdash_J B, \\ \text{F} & \text{if } \not\vdash_J B. \end{cases}$$

     *(Because $J$ is consistent and complete, for every wf. $B$, **exactly one** of $\vdash_J B$ or $\vdash_J (\sim B)$ holds! Thus $v(B) = \text{F} \iff \vdash_J (\sim B)$.)*
  4. **Verify that $v$ is a legitimate valuation of $L$ (Definition 2.4)**:
     * **Condition 1 ($v((\sim B)) \ne v(B)$)**: Since $J$ is consistent and complete, $\vdash_J (\sim B) \iff \not\vdash_J B$, so $v((\sim B)) = \text{T} \iff v(B) = \text{F}$.
     * **Condition 2 ($v((B \to C)) = \text{F} \iff v(B) = \text{T}$ and $v(C) = \text{F}$)**:
       * $(\Leftarrow)$ Suppose $v(B) = \text{T}$ and $v(C) = \text{F}$. Then $\vdash_J B$ and $\vdash_J (\sim C)$. By Lemma 2.8(3) ($\{B, (\sim C)\} \vdash_L \sim(B \to C)$), we get $\vdash_J \sim(B \to C)$, so by consistency $\not\vdash_J (B \to C)$, i.e., $v((B \to C)) = \text{F}$.
       * $(\Rightarrow)$ Suppose $v(B) = \text{F}$ or $v(C) = \text{T}$.
         * If $v(B) = \text{F}$, then $\vdash_J (\sim B)$, so by Lemma 2.8(1) ($\{(\sim B)\} \vdash_L (B \to C)$) we have $\vdash_J (B \to C)$, meaning $v((B \to C)) = \text{T}$.
         * If $v(C) = \text{T}$, then $\vdash_J C$, so by Lemma 2.8(2) ($\{C\} \vdash_L (B \to C)$) we have $\vdash_J (B \to C)$, meaning $v((B \to C)) = \text{T}$.
       Thus $v$ is a valuation of $L$!
  5. **Evaluate $v(A)$**: Since $(\sim A)$ was added as an axiom of $L^* \subseteq J$, we have $\vdash_J (\sim A)$. Because $J$ is consistent, $\not\vdash_J A$, so $v(A) = \text{F}$. Therefore $A$ is **not** a tautology! By contraposition, if $\models_L A$, then $\vdash_L A$ ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)). $\blacksquare$

### 🏆 Corollaries of Soundness + Adequacy (Compactness & Decidability)
Combining Soundness (Proposition 2.10) and Adequacy (Proposition 2.13), for any wf. $A$ of $L$ (and any set of hypotheses $\Gamma$) ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)):

$$\vdash_L A \iff \models_L A \qquad \text{and} \qquad \Gamma \vdash_L A \iff \Gamma \models_L A.$$

* **Corollary 2.17 (Compactness Theorem for $L$)**: If $\Gamma \models_L A$, then there is a **finite** subset $\Gamma_0 \subseteq \Gamma$ such that $\Gamma_0 \models_L A$. Equivalently, a set $\Gamma$ of wfs. of $L$ has a model (a valuation $v$ making all wfs. in $\Gamma$ true) if and only if every **finite** subset of $\Gamma$ has a model ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).
* **Corollary 2.18 (Decidability of $L$)**: There is an effective mechanical algorithm (namely, constructing the $2^n$-row truth table!) to decide in finite time whether any given wf. $A$ of $L$ is a theorem of $L$ ($\vdash_L A$) ([Cambridge Logic & Set Theory Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf)).

> 📝 **My Study Notes (Section 2.2)**:
> * *Key takeaway to remember:*

# ⚡ Rapid Midterm Review Sheet (Chapters 1 & 2 At a Glance)

| Concept / Theorem | Symbolic Statement | One-Line Proof Idea / Exam Trigger | Reference |
| :--- | :--- | :--- | :--- |
| **Number of $n$-place truth functions** | $2^{(2^n)}$ | $2^n$ truth-table rows, $2$ choices ($\text{T}$ or $\text{F}$) per row | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| **Uniform Substitution (Prop. 1.2)** | $A(p_1, \dots, p_n)$ tautology $\implies A(A_1, \dots, A_n)$ tautology | Must replace **every** occurrence of $p_i$ by $A_i$ | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| **Subformula Replacement (Prop. 1.3)** | $A_1 \equiv A_2 \implies B[\text{some } A_1 / A_2] \equiv B$ | Can replace **some or all** occurrences of $A_1$ | [Logic of Antinomies](https://projecteuclid.org/journals/notre-dame-journal-of-formal-logic/volume-16/issue-1/Logic-of-antinomies/10.1305/ndjfl/1093891610.pdf) |
| **DNF (Cor. 1.6) & CNF (Cor. 1.7)** | $\bigvee_r \bigwedge_j Q_{rj}$ (non-contradiction) <br> $\bigwedge_r \bigvee_j Q_{rj}$ (non-tautology) | DNF reads $\text{T}$-rows of $A$; CNF negates DNF of $\sim A$ via De Morgan | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| **Adequate Sets (Cor. 1.9 & Prop. 1.10)** | $\{\sim,\wedge,\vee\}, \{\sim,\wedge\}, \{\sim,\vee\}, \{\sim,\to\}, \{\downarrow\}, \{\mid\}$ | Only $\downarrow$ (NOR) and $\mid$ (NAND) work as singletons | [Note 1/7](https://medium.com/@chengjing/logic-for-mathematicians-note-1-7-272f5c95ecf8) |
| **Argument Validity (Prop. 1.11)** | $A_1, \dots, A_n \therefore B$ valid $\iff ((A_1 \wedge \dots \wedge A_n) \to B)$ is a tautology | Test by setting $A_1 = \dots = A_n = \text{T}$ and $B = \text{F}$ | [Features of Propositional Logic](https://www.m-hikari.com/pms/pms-2021/pms-1-2021/p/alodhariPMS1-2021.pdf) |
| **Axioms of $L$** | **(L1)** $A \to (B \to A)$ <br> **(L2)** $(A \to (B \to C)) \to ((A \to B) \to (A \to C))$ <br> **(L3)** $((\sim A) \to (\sim B)) \to (B \to A)$ | Rule of inference: **MP** ($A, A \to B \vdash B$); $(A \vee B) := ((\sim A) \to B)$ | [ViXra Reference](https://vixra.org/pdf/1904.0095v1.pdf) |
| **Identity Theorem (Lemma 2.1)** | $\vdash_L (A \to A)$ | 5 lines: (L2) with $B = A \to A, C = A$; then (L1), MP, (L1), MP | [ViXra Reference](https://vixra.org/pdf/1904.0095v1.pdf) |
| **Deduction Theorem (Prop. 2.2)** | $\Gamma \cup \{A\} \vdash_L B \implies \Gamma \vdash_L (A \to B)$ | Induction on deduction length $m$: Case 1 uses (L1), Case 2 uses $A \to A$, Case 3 uses (L2) | [Cambridge Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf) |
| **Soundness Theorem (Prop. 2.10)** | $\vdash_L A \implies \models_L A$ | (L1)–(L3) are tautologies and MP preserves $\text{T}$; implies $L$ is consistent | [Cambridge Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf) |
| **Adjoining Non-Theorem (Prop. 2.15)** | $L^*$ consistent & $\not\vdash_{L^*} A \implies L^* + (\sim A)$ consistent | If inconsistent, $\{\sim A\} \vdash_{L^*} B, \sim B$, so DT + Reductio gives $\vdash_{L^*} A$ | [Math SE p. 41](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l) |
| **Lindenbaum's Lemma (Prop. 2.16)** | $L^*$ consistent $\implies \exists J \supseteq L^*$ consistent & complete | Enumerate wfs. $A_0, A_1, \dots$; add $\sim A_n$ whenever $\not\vdash_{J_n} A_n$; union is consistent by finite proof length | [Math SE p. 41](https://math.stackexchange.com/questions/768609/show-that-any-consistent-extension-l-of-l-has-a-consistent-extension-l) |
| **Adequacy Theorem (Prop. 2.13)** | $\models_L A \implies \vdash_L A$ | Proved via Kalmár's Lemma $\{p_1',\dots,p_n'\} \vdash_L A'$ OR via Lindenbaum's complete extension $J$ | [Cambridge Notes](https://tartarus.org/gareth/maths/notes/ii/Logic_and_Set_Theory.pdf) |
