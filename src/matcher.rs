use std::{iter::Copied, ops::Range, slice::Iter};

use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use itertools::Either;
const BYTES_1M: usize = 1024 * 1024 * 1024;

struct Term {
    pattern: String,
}

struct ParsedQuery {
    terms: Vec<Term>,
}

impl ParsedQuery {
    fn parse(query: &str) -> Self {
        let terms = query
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .map(|p| Term {
                pattern: p.to_string(),
            })
            .collect();
        ParsedQuery { terms }
    }
}

pub struct FuzzyEngine {
    parsed_query: ParsedQuery,
    matcher: SkimMatcherV2,
}

impl FuzzyEngine {
    pub fn new(query: String) -> Self {
        let matcher = SkimMatcherV2::default().element_limit(BYTES_1M);
        let matcher = matcher.smart_case();
        let parsed_query = ParsedQuery::parse(&query);
        FuzzyEngine {
            matcher,
            parsed_query,
        }
    }

    pub fn match_line(&self, line: &str) -> (i64, Vec<usize>) {
        if self.parsed_query.terms.is_empty() {
            return (0, vec![]);
        }

        let mut total_score: i64 = 0;
        let mut all_indices: Vec<usize> = Vec::new();

        for term in &self.parsed_query.terms {
            if let Some((score, indices)) = self.matcher.fuzzy_indices(line, &term.pattern) {
                total_score = total_score.saturating_add(score);
                all_indices.extend(indices);
            } else {
                return (0, vec![]);
            }
        }

        all_indices.sort_unstable();
        all_indices.dedup();

        (total_score, all_indices)
    }
}

pub struct FuzzyIndex {
    /// indices into [`App.history`]
    indices: Option<Vec<usize>>,
    /// scores parallel to indices
    scores: Option<Vec<i64>>,
    /// highlight matches, globally indexed, parallel to [`App.history`]
    highlight_indices: Option<Vec<Vec<usize>>>,
}

impl FuzzyIndex {
    /// creates an identity mapping
    pub fn identity() -> Self {
        Self {
            indices: None,
            scores: None,
            highlight_indices: None,
        }
    }

    /// crates an index from a matcher result
    pub fn new(scored_indices: Vec<(usize, i64)>, highlight_indices: Vec<Vec<usize>>) -> Self {
        let (indices, scores) = scored_indices.into_iter().unzip();
        Self {
            indices: Some(indices),
            scores: Some(scores),
            highlight_indices: Some(highlight_indices),
        }
    }

    /// number of matches or None (if all)
    pub fn len(&self) -> Option<usize> {
        self.indices.as_ref().map(|ind| ind.len())
    }

    /// gets the first n indices
    pub fn first_n(&self, n: usize) -> Either<Copied<Iter<'_, usize>>, Range<usize>> {
        if let Some(indices) = &self.indices {
            let visible_count = n.min(indices.len());
            #[allow(clippy::indexing_slicing)] // slicing: using min ensures the slice is valid
            Either::Left(indices[0..visible_count].iter().copied())
        } else {
            Either::Right(0..n)
        }
    }

    /// get the i-th index
    pub fn get(&self, index: usize) -> Option<usize> {
        if let Some(indices) = &self.indices {
            indices.get(index).copied()
        } else {
            Some(index)
        }
    }

    pub fn matcher_score(&self, index: usize) -> Option<i64> {
        if let Some(scores) = &self.scores {
            scores.get(index).copied()
        } else {
            None
        }
    }

    /// get the highlight indices
    pub fn highlight_indices(&self, index: usize) -> Option<&Vec<usize>> {
        if let Some(indices) = &self.highlight_indices {
            indices.get(index)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_term_matches() {
        let engine = FuzzyEngine::new("git".to_string());
        let (score, indices) = engine.match_line("git commit");
        assert!(score > 0);
        assert!(!indices.is_empty());
    }

    #[test]
    fn single_term_no_match() {
        let engine = FuzzyEngine::new("xyz".to_string());
        let (score, indices) = engine.match_line("git commit");
        assert_eq!(score, 0);
        assert!(indices.is_empty());
    }

    #[test]
    fn and_both_terms_match() {
        let engine = FuzzyEngine::new("git commit".to_string());
        let (score, indices) = engine.match_line("git commit -m 'message'");
        assert!(score > 0);
        assert!(!indices.is_empty());
    }

    #[test]
    fn and_one_term_fails() {
        let engine = FuzzyEngine::new("git xyz".to_string());
        let (score, indices) = engine.match_line("git commit");
        assert_eq!(score, 0);
        assert!(indices.is_empty());
    }

    #[test]
    fn empty_query_no_match() {
        let engine = FuzzyEngine::new("".to_string());
        let (score, indices) = engine.match_line("git commit");
        assert_eq!(score, 0);
        assert!(indices.is_empty());
    }

    #[test]
    fn whitespace_only_query_no_match() {
        let engine = FuzzyEngine::new("   ".to_string());
        let (score, indices) = engine.match_line("git commit");
        assert_eq!(score, 0);
        assert!(indices.is_empty());
    }

    #[test]
    fn highlight_indices_deduplicated() {
        let engine = FuzzyEngine::new("ab ba".to_string());
        let (score, indices) = engine.match_line("abba");
        assert!(score > 0);
        let mut sorted_indices = indices.clone();
        sorted_indices.sort_unstable();
        sorted_indices.dedup();
        assert_eq!(indices, sorted_indices, "indices should be sorted and deduplicated");
    }

    #[test]
    fn multiple_terms_score_accumulates() {
        let engine_single = FuzzyEngine::new("git".to_string());
        let engine_double = FuzzyEngine::new("git commit".to_string());

        let (score_single, _) = engine_single.match_line("git commit -m 'message'");
        let (score_double, _) = engine_double.match_line("git commit -m 'message'");

        assert!(
            score_double > score_single,
            "two matching terms should have higher score than one"
        );
    }
}
