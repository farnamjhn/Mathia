# Group Theory Notes

These notes cover the first results on finite groups. Definitions vary between authors.

## Basic notions

**Definition 1.1.** A *group* is a set $G$ with an operation $\cdot$ such that $(G, \cdot)$ is associative, has an identity $e$, and every element has an inverse. We say $G$ is *abelian* if

$$\forall a, b \in G: \; ab = ba.$$

Example 1.2. The integers $(\mathbb{Z}, +)$ form an abelian group.

Note that the operation is usually written multiplicatively.

## Lagrange

**Theorem 1.3 (Lagrange).** If $H$ is a subgroup of a finite group $G$, then

$$|G| = [G : H] \cdot |H|,$$

where $[G:H]$ is the number of left cosets of $H$ in $G$.

*Proof.* Suppose $aH$ and $bH$ are two left cosets. The map $h \mapsto ah$ is a bijection $H \to aH$, so every coset has $|H|$ elements.

The cosets partition $G$: if $aH \cap bH \neq \emptyset$ then $aH = bH$. Hence $|G|$ is the sum of the sizes of $[G:H]$ disjoint cosets. $\square$

By Theorem 1.3 the order of a subgroup always divides the order of the group.

**Corollary 1.4.** The order of every element of a finite group $G$ divides $|G|$.

**Proposition 1.5.** Every group of prime order is cyclic.

Proof of Proposition 1.5. Take $a \neq e$; by Corollary 1.4 the order of $a$ is $p$. Q.E.D.

**Lemma 1.6.** A subgroup of index $2$ is normal.

**Remark.** The converse of Lagrange's theorem is false in general.
