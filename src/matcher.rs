use std::{iter::Copied, ops::Range, slice::Iter};

use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use itertools::Either;
use parser::ParsedQuery;

const BYTES_1M: usize = 1024 * 1024 * 1024;

/// Query parser for fuzzy matching with AND/OR combinators.
///
/// Grammar:
/// - `foo bar` → foo AND bar (space = AND)
/// - `foo | bar` → foo OR bar (` | ` = OR, must have spaces)
/// - OR binds tighter: `a b | c` → a AND (b OR c)
mod parser {
    use nom::{
        IResult, Parser,
        bytes::complete::{tag, take_while1},
        character::complete::space1,
        combinator::map,
        multi::separated_list1,
    };

    /// A single search term (non-whitespace sequence).
    pub struct Term {
        pub pattern: String,
    }

    /// Terms connected by OR (` | `). Any term matching = group matches.
    pub struct OrGroup {
        pub terms: Vec<Term>,
    }

    /// Groups connected by AND (space). All groups must match.
    pub struct ParsedQuery {
        pub groups: Vec<OrGroup>,
    }

    /// Parses a single non-whitespace token.
    fn parse_term(input: &str) -> IResult<&str, Term> {
        map(take_while1(|c: char| !c.is_whitespace()), |s: &str| Term {
            pattern: s.to_string(),
        })
        .parse(input)
    }

    /// Parses terms separated by ` | ` into an OR group.
    fn parse_or_group(input: &str) -> IResult<&str, OrGroup> {
        map(separated_list1(tag(" | "), parse_term), |terms| OrGroup {
            terms,
        })
        .parse(input)
    }

    /// Parses OR groups separated by whitespace (AND semantics).
    fn parse_groups(input: &str) -> IResult<&str, Vec<OrGroup>> {
        separated_list1(space1, parse_or_group).parse(input)
    }

    impl ParsedQuery {
        pub fn parse(query: &str) -> Self {
            let input = query.trim();
            if input.is_empty() {
                return ParsedQuery { groups: vec![] };
            }
            match parse_groups(input) {
                Ok((_, groups)) => ParsedQuery { groups },
                Err(_) => ParsedQuery { groups: vec![] },
            }
        }
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
        if self.parsed_query.groups.is_empty() {
            return (0, vec![]);
        }

        let mut total_score: i64 = 0;
        let mut all_indices: Vec<usize> = Vec::new();

        for group in &self.parsed_query.groups {
            let mut group_matched = false;

            for term in &group.terms {
                if let Some((score, indices)) = self.matcher.fuzzy_indices(line, &term.pattern) {
                    total_score = total_score.saturating_add(score);
                    all_indices.extend(indices);
                    group_matched = true;
                    break;
                }
            }

            if !group_matched {
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
        assert_eq!(
            indices, sorted_indices,
            "indices should be sorted and deduplicated"
        );
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

    #[test]
    fn or_first_alternative_matches() {
        let engine = FuzzyEngine::new("git | hg".to_string());
        let (score, indices) = engine.match_line("git commit");
        assert!(score > 0);
        assert!(!indices.is_empty());
    }

    #[test]
    fn or_second_alternative_matches() {
        let engine = FuzzyEngine::new("git | hg".to_string());
        let (score, indices) = engine.match_line("hg commit");
        assert!(score > 0);
        assert!(!indices.is_empty());
    }

    #[test]
    fn or_no_alternative_matches() {
        let engine = FuzzyEngine::new("git | hg".to_string());
        let (score, indices) = engine.match_line("svn commit");
        assert_eq!(score, 0);
        assert!(indices.is_empty());
    }

    #[test]
    fn or_with_and() {
        let engine = FuzzyEngine::new("git | hg commit".to_string());

        let (score1, _) = engine.match_line("git commit");
        assert!(score1 > 0, "git commit should match");

        let (score2, _) = engine.match_line("hg commit");
        assert!(score2 > 0, "hg commit should match");

        let (score3, _) = engine.match_line("git push");
        assert_eq!(score3, 0, "git push should not match (missing commit)");

        let (score4, _) = engine.match_line("svn commit");
        assert_eq!(score4, 0, "svn commit should not match (missing git|hg)");
    }

    #[test]
    fn or_binds_tighter_than_and() {
        let engine = FuzzyEngine::new("readme .md | .txt".to_string());

        let (score1, _) = engine.match_line("readme.md");
        assert!(score1 > 0, "readme.md should match");

        let (score2, _) = engine.match_line("readme.txt");
        assert!(score2 > 0, "readme.txt should match");

        let (score3, _) = engine.match_line("readme.rs");
        assert_eq!(score3, 0, "readme.rs should not match");

        let (score4, _) = engine.match_line("changelog.md");
        assert_eq!(score4, 0, "changelog.md should not match (missing readme)");

        // This is the key precedence test: if AND bound tighter, "notes.txt" would match
        // because it would parse as (readme AND .md) OR .txt
        let (score5, _) = engine.match_line("notes.txt");
        assert_eq!(
            score5, 0,
            "notes.txt should not match (OR binds tighter than AND)"
        );
    }

    #[test]
    fn trailing_pipe_is_literal() {
        let engine = FuzzyEngine::new("git |".to_string());

        let (score1, _) = engine.match_line("git | wc");
        assert!(score1 > 0, "should match git AND |");

        let (score2, _) = engine.match_line("git commit");
        assert_eq!(score2, 0, "should not match - missing |");
    }

    #[test]
    fn leading_pipe_is_literal() {
        let engine = FuzzyEngine::new("| git".to_string());

        let (score1, _) = engine.match_line("foo | git bar");
        assert!(score1 > 0, "should match | AND git");

        let (score2, _) = engine.match_line("git commit");
        assert_eq!(score2, 0, "should not match - missing |");
    }

    #[test]
    fn lone_pipe_is_literal() {
        let engine = FuzzyEngine::new("|".to_string());

        let (score1, _) = engine.match_line("foo | bar");
        assert!(score1 > 0, "should match literal |");

        let (score2, _) = engine.match_line("foo bar");
        assert_eq!(score2, 0, "should not match - no pipe");
    }

    #[test]
    fn pipe_without_spaces_is_literal() {
        let engine = FuzzyEngine::new("grep|wc".to_string());

        let (score1, _) = engine.match_line("cat file | grep foo | wc -l");
        assert!(score1 > 0, "should match literal grep|wc");

        let (score2, _) = engine.match_line("grep something");
        assert_eq!(score2, 0, "should not match grep alone (no pipe)");
    }
}
