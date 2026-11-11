# Measure Theory, Lecture 2

\begin{definition}
A collection $\mathcal{A}$ of subsets of $X$ is a *$\sigma$-algebra* if it contains $X$ and is closed under complements and countable unions.
\end{definition}

**Notation.** We write $L^1(\mu)$ for the space of $\mu$-integrable functions.

## Convergence theorems

\begin{theorem}[Monotone convergence]
Let $(f_n)$ be an increasing sequence of non-negative measurable functions with pointwise limit $f$. Then

\begin{align}
\int f \, d\mu &= \lim_{n \to \infty} \int f_n \, d\mu.
\end{align}

The limit may be infinite.
\end{theorem}

\begin{proof}
By monotonicity of the integral the limit exists and is at most $\int f \, d\mu$.

The reverse inequality follows by approximating $f$ from below by simple functions.
\end{proof}

Claim 2.3. Every simple function is measurable.

Proof of Claim 2.3. Immediate from the definition, since a simple function is a finite sum of indicators of measurable sets. □

Remark 2.5 is false without monotonicity; see \[1\].

Exercise 2.4. Show that the conclusion of the theorem fails for decreasing sequences.

Solution. Take $f_n = \mathbf{1}_{[n, \infty)}$ on $\mathbb{R}$ with Lebesgue measure. Each $f_n$ has infinite integral, yet $f_n \to 0$ pointwise. Hence limit and integral cannot be exchanged. Q.E.D.

By Claim 2.3 the approximation in the proof above is legitimate.
