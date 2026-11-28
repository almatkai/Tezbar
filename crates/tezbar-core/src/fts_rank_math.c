/* SQLite 3.46.0 BM25 arithmetic, public domain.
 * Compile with the same cc defaults/profile as bundled SQLite. Do not rewrite
 * this in Rust or add fast-math: C contraction/vectorization is observable in
 * the exact score/tie oracle (including the vector-prefix/scalar-tail boundary).
 */
double tezbar_bm25_sum(int nPhrase, const double *aIDF, const double *aFreq,
                       double D, double avgdl) {
    const double k1 = 1.2;
    const double b = 0.75;
    double score = 0.0;
    int i;
    for (i = 0; i < nPhrase; i++) {
        score += aIDF[i] * (
            (aFreq[i] * (k1 + 1.0)) /
            (aFreq[i] + k1 * (1 - b + b * D / avgdl))
        );
    }
    return -1.0 * score;
}
