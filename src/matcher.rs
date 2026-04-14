use std::{iter::Copied, ops::Range, slice::Iter};

use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use itertools::Either;
use parser::{OrGroup, ParsedQuery, Term, TermType};

const BYTES_1M: usize = 1024 * 1024 * 1024;

/// Query parser for fuzzy matching with AND/OR combinators.
///
/// Grammar:
/// - `foo bar` -> foo AND bar (space = AND)
/// - `foo | bar` -> foo OR bar (` | ` = OR, must have spaces)
/// - OR binds tighter: `a b | c` -> a AND (b OR c)
///
/// Special operators (prefixes/suffixes):
/// - `^foo` -> prefix match (line starts with "foo")
/// - `foo$` -> suffix match (line ends with "foo")
/// - `'foo` -> exact substring match (no fuzzy)
/// - `!foo` -> inverse exact match (must NOT contain "foo")
/// - `!foo$` -> inverse suffix match
mod parser {
    use nom::{
        IResult, Parser,
        branch::alt,
        bytes::complete::{tag, take_while1},
        character::complete::space1,
        combinator::{map, rest},
        multi::separated_list1,
        sequence::preceded,
    };

    /// How to match a term against the line.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TermType {
        /// Default fuzzy matching
        Fuzzy,
        /// `^pattern` - line must start with pattern
        Prefix,
        /// `pattern$` - line must end with pattern
        Suffix,
        /// `'pattern` - exact substring match (not fuzzy)
        Exact,
        /// `!pattern` - line must NOT contain pattern
        InverseExact,
        /// `!pattern$` - line must NOT end with pattern
        InverseSuffix,
    }

    /// A single search term with its match type.
    pub struct Term {
        pub pattern: String,
        pub term_type: TermType,
    }

    /// Parses `^pattern` -> Prefix
    fn parse_prefix(input: &str) -> IResult<&str, Term> {
        map(preceded(tag("^"), rest), |pattern: &str| Term {
            pattern: pattern.to_string(),
            term_type: TermType::Prefix,
        })
        .parse(input)
    }

    /// Parses `'pattern` -> Exact
    fn parse_exact(input: &str) -> IResult<&str, Term> {
        map(preceded(tag("'"), rest), |pattern: &str| Term {
            pattern: pattern.to_string(),
            term_type: TermType::Exact,
        })
        .parse(input)
    }

    /// Parses `!pattern` -> InverseExact
    fn parse_inverse_exact(input: &str) -> IResult<&str, Term> {
        map(preceded(tag("!"), rest), |pattern: &str| Term {
            pattern: pattern.to_string(),
            term_type: TermType::InverseExact,
        })
        .parse(input)
    }

    /// Parses `pattern$` -> Suffix (pattern must be non-empty)
    fn parse_suffix(input: &str) -> IResult<&str, Term> {
        if !input.ends_with('$') || input.len() < 2 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
        let pattern = &input[..input.len() - 1];
        Ok((
            "",
            Term {
                pattern: pattern.to_string(),
                term_type: TermType::Suffix,
            },
        ))
    }

    /// Parses `!pattern$` -> InverseSuffix (must not end with pattern)
    fn parse_inverse_suffix(input: &str) -> IResult<&str, Term> {
        let (rest, _) = tag::<_, _, nom::error::Error<&str>>("!")(input)?;
        if !rest.ends_with('$') || rest.len() < 2 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }
        let pattern = &rest[..rest.len() - 1];
        Ok((
            "",
            Term {
                pattern: pattern.to_string(),
                term_type: TermType::InverseSuffix,
            },
        ))
    }

    /// Parses plain `pattern` -> Fuzzy
    fn parse_fuzzy(input: &str) -> IResult<&str, Term> {
        Ok((
            "",
            Term {
                pattern: input.to_string(),
                term_type: TermType::Fuzzy,
            },
        ))
    }

    /// Parse a raw token into a Term, extracting any operator prefix/suffix.
    fn parse_term_type(input: &str) -> Term {
        #[allow(clippy::expect_used)]
        alt((
            parse_inverse_suffix,
            parse_inverse_exact,
            parse_prefix,
            parse_exact,
            parse_suffix,
            parse_fuzzy,
        ))
        .parse(input)
        .map(|(_, term)| term)
        .expect("parse_fuzzy is a catch-all that always succeeds")
    }

    /// Terms connected by OR (` | `). Any term matching = group matches.
    pub struct OrGroup {
        pub terms: Vec<Term>,
    }

    /// Groups connected by AND (space). All groups must match.
    pub struct ParsedQuery {
        pub groups: Vec<OrGroup>,
    }

    /// Parses a single non-whitespace token into a Term.
    fn parse_term(input: &str) -> IResult<&str, Term> {
        map(take_while1(|c: char| !c.is_whitespace()), |s: &str| {
            parse_term_type(s)
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

    /// Match a single term against the line, returning (score, indices) or None.
    /// We match Skims behavior here and use the pattern length as a score for non-fuzzy matcher.
    fn match_term(&self, line: &str, term: &Term) -> Option<(i64, Vec<usize>)> {
        match term.term_type {
            TermType::Fuzzy => self.matcher.fuzzy_indices(line, &term.pattern),

            TermType::Exact => {
                let start = line.find(&term.pattern)?;
                let indices: Vec<usize> = (start..start + term.pattern.len()).collect();
                Some((term.pattern.len() as i64, indices))
            }

            TermType::Prefix => {
                if line.starts_with(&term.pattern) {
                    let indices: Vec<usize> = (0..term.pattern.len()).collect();
                    Some((term.pattern.len() as i64, indices))
                } else {
                    None
                }
            }

            TermType::Suffix => {
                if line.ends_with(&term.pattern) {
                    let start = line.len() - term.pattern.len();
                    let indices: Vec<usize> = (start..line.len()).collect();
                    Some((term.pattern.len() as i64, indices))
                } else {
                    None
                }
            }

            // TODO: match_line returns (0, vec![]) for "no match", so we use score 1 here
            // to distinguish "inverse matched" from "no match". Consider introducing:
            //   enum MatchResult { NoMatch, Match { score: i64, indices: Vec<usize> } }
            TermType::InverseExact => {
                if line.contains(&term.pattern) {
                    None
                } else {
                    Some((1, vec![]))
                }
            }

            TermType::InverseSuffix => {
                if line.ends_with(&term.pattern) {
                    None
                } else {
                    Some((1, vec![]))
                }
            }
        }
    }

    /// Match an OR group: returns first matching term's (score, indices), or None if no term matches.
    fn match_or_group(&self, line: &str, group: &OrGroup) -> Option<(i64, Vec<usize>)> {
        group
            .terms
            .iter()
            .find_map(|term| self.match_term(line, term))
    }

    pub fn match_line(&self, line: &str) -> (i64, Vec<usize>) {
        if self.parsed_query.groups.is_empty() {
            return (0, vec![]);
        }

        // AND: all groups must match
        let group_results: Option<Vec<_>> = self
            .parsed_query
            .groups
            .iter()
            .map(|group| self.match_or_group(line, group))
            .collect();

        let Some(results) = group_results else {
            return (0, vec![]);
        };

        let total_score = results.iter().map(|(score, _)| score).sum();
        let mut all_indices: Vec<usize> = results
            .into_iter()
            .flat_map(|(_, indices)| indices)
            .collect();
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

    /// crates an index from a sorted matcher result
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

    pub fn is_empty(&self) -> bool {
        self.len().is_none()
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
    fn fuzzy_index_identity_get() {
        let index = FuzzyIndex::identity();
        assert_eq!(index.get(0), Some(0));
        assert_eq!(index.get(42), Some(42));
    }

    #[test]
    fn fuzzy_index_identity_len_and_empty() {
        let index = FuzzyIndex::identity();
        assert_eq!(index.len(), None);
        assert!(index.is_empty());
    }

    #[test]
    fn fuzzy_index_identity_first_n() {
        let index = FuzzyIndex::identity();
        let result: Vec<usize> = index.first_n(3).collect();
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn fuzzy_index_identity_no_highlights_or_scores() {
        let index = FuzzyIndex::identity();
        assert_eq!(index.highlight_indices(0), None);
        assert_eq!(index.matcher_score(0), None);
    }

    #[test]
    fn fuzzy_index_filtered_get() {
        let index = FuzzyIndex::new(vec![(5, 100), (2, 50)], vec![vec![0, 1], vec![3]]);
        assert_eq!(index.get(0), Some(5));
        assert_eq!(index.get(1), Some(2));
        assert_eq!(index.get(2), None);
    }

    #[test]
    fn fuzzy_index_filtered_len() {
        let index = FuzzyIndex::new(vec![(5, 100), (2, 50)], vec![vec![], vec![]]);
        assert_eq!(index.len(), Some(2));
        assert!(!index.is_empty());
    }

    #[test]
    fn fuzzy_index_filtered_first_n() {
        let index = FuzzyIndex::new(
            vec![(5, 100), (2, 50), (8, 10)],
            vec![vec![], vec![], vec![]],
        );
        let result: Vec<usize> = index.first_n(2).collect();
        assert_eq!(result, vec![5, 2]);
    }

    #[test]
    fn fuzzy_index_filtered_first_n_clamps_to_available() {
        let index = FuzzyIndex::new(vec![(1, 10)], vec![vec![]]);
        let result: Vec<usize> = index.first_n(10).collect();
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn fuzzy_index_filtered_scores_and_highlights() {
        let index = FuzzyIndex::new(vec![(5, 100), (2, 50)], vec![vec![0, 1], vec![3, 4]]);
        assert_eq!(index.matcher_score(0), Some(100));
        assert_eq!(index.matcher_score(1), Some(50));
        assert_eq!(index.matcher_score(2), None);
        assert_eq!(index.highlight_indices(0), Some(&vec![0, 1]));
        assert_eq!(index.highlight_indices(1), Some(&vec![3, 4]));
        assert_eq!(index.highlight_indices(2), None);
    }

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

    // Phase 3: Special operators

    #[test]
    fn prefix_match() {
        let engine = FuzzyEngine::new("^git".to_string());

        let (score1, indices) = engine.match_line("git commit");
        assert!(score1 > 0, "should match - starts with git");
        assert_eq!(indices, vec![0, 1, 2]);

        let (score2, _) = engine.match_line("fugitive git");
        assert_eq!(score2, 0, "should not match - git not at start");
    }

    #[test]
    fn suffix_match() {
        let engine = FuzzyEngine::new(".rs$".to_string());

        let (score1, indices) = engine.match_line("main.rs");
        assert!(score1 > 0, "should match - ends with .rs");
        assert_eq!(indices, vec![4, 5, 6]);

        let (score2, _) = engine.match_line("main.rs.bak");
        assert_eq!(score2, 0, "should not match - .rs not at end");
    }

    #[test]
    fn exact_match() {
        let engine = FuzzyEngine::new("'git".to_string());

        let (score1, _) = engine.match_line("git commit");
        assert!(score1 > 0, "should match - contains git");

        let (score2, _) = engine.match_line("gti commit");
        assert_eq!(score2, 0, "should not match - gti is not git (no fuzzy)");
    }

    #[test]
    fn inverse_exact_match() {
        let engine = FuzzyEngine::new("!test".to_string());

        let (score1, _) = engine.match_line("cargo build");
        assert!(score1 > 0, "should match - does not contain test");

        let (score2, _) = engine.match_line("cargo test");
        assert_eq!(score2, 0, "should not match - contains test");
    }

    #[test]
    fn inverse_suffix_match() {
        let engine = FuzzyEngine::new("!.tmp$".to_string());

        let (score1, _) = engine.match_line("main.rs");
        assert!(score1 > 0, "should match - does not end with .tmp");

        let (score2, _) = engine.match_line("file.tmp");
        assert_eq!(score2, 0, "should not match - ends with .tmp");
    }

    #[test]
    fn combined_operators() {
        let engine = FuzzyEngine::new("^git !test".to_string());

        let (score1, _) = engine.match_line("git commit");
        assert!(score1 > 0, "should match - starts with git, no test");

        let (score2, _) = engine.match_line("git test");
        assert_eq!(score2, 0, "should not match - contains test");

        let (score3, _) = engine.match_line("fugitive git");
        assert_eq!(score3, 0, "should not match - doesn't start with git");
    }

    #[test]
    fn operators_with_or() {
        let engine = FuzzyEngine::new(".rs$ | .py$".to_string());

        let (score1, _) = engine.match_line("main.rs");
        assert!(score1 > 0, "should match .rs");

        let (score2, _) = engine.match_line("main.py");
        assert!(score2 > 0, "should match .py");

        let (score3, _) = engine.match_line("main.go");
        assert_eq!(score3, 0, "should not match .go");
    }
}
