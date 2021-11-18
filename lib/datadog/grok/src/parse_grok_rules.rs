use std::{collections::HashMap, convert::TryFrom, fmt::Write, sync::Arc};

use grok::Grok;
use lazy_static::lazy_static;

use lookup::LookupBuf;

use crate::matchers::date::DateFilter;
use crate::{
    ast::{self, Destination, GrokPattern},
    grok_filter::GrokFilter,
    matchers::date,
    parse_grok_pattern::parse_grok_pattern,
};
use itertools::{Itertools, Position};
use regex::Regex;
use std::collections::BTreeMap;
use vrl_compiler::Value;

#[derive(Debug, Clone)]
pub struct GrokRule {
    pub pattern: Arc<grok::Pattern>,
    pub filters: HashMap<LookupBuf, Vec<GrokFilter>>,
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("failed to parse grok expression '{}': {}", .0, .1)]
    InvalidGrokExpression(String, String),
    #[error("invalid arguments for the function '{}'", .0)]
    InvalidFunctionArguments(String),
    #[error("unknown filter '{}'", .0)]
    UnknownFilter(String),
    #[error("Circular dependency found in the alias '{}'", .0)]
    CircularDependencyInAliasDefinition(String),
}

///
/// Parses DD grok rules.
///
/// Here is an example:
/// patterns:
///  %{access.common} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)
///  %{access.common} (%{number:duration:scale(1000000000)} )?"%{_referer}" "%{_user_agent}"( "%{_x_forwarded_for}")?.*"#
/// aliases:
///  "access.common" : %{_client_ip} %{_ident} %{_auth}
///
/// You can write grok patterns with the %{MATCHER:EXTRACT:FILTER} syntax:
/// - Matcher: A rule (possibly a reference to another token rule) that describes what to expect (number, word, notSpace, etc.)
/// - Extract (optional): An identifier representing the capture destination for the piece of text matched by the Matcher.
/// - Filter (optional): A post-processor of the match to transform it.
///
/// Rules can reference aliases as %{alias_name}, aliases can reference each other themselves, cross-references or circular dependencies are not allowed and result in an error.
/// Only one can match any given log. The first one that matches, from top to bottom, is the one that does the parsing.
/// For further documentation and the full list of available matcher and filters check out https://docs.datadoghq.com/logs/processing/parsing
pub fn parse_grok_rules(
    patterns: &[String],
    aliases: BTreeMap<String, String>,
) -> Result<Vec<GrokRule>, Error> {
    let mut parsed_aliases: HashMap<String, ParsedGrokRule> = HashMap::new();
    let mut inflight_parsed_aliases = vec![];

    for (name, alias) in &aliases {
        if !alias.is_empty() {
            let parsed_alias = parse_alias(
                name,
                alias,
                &aliases,
                &mut parsed_aliases,
                &mut inflight_parsed_aliases,
            )?;
            parsed_aliases.insert(name.to_string(), parsed_alias);
        }
    }

    let mut grok = initialize_grok();

    patterns
        .iter()
        .filter(|&r| !r.is_empty())
        .map(|r| parse_pattern(r, &aliases, &mut parsed_aliases, &mut grok))
        .collect::<Result<Vec<GrokRule>, Error>>()
}

/// The result of parsing grok rules - pure grok definitions, which can be feed directly to the grok,
/// and rule filters to post-process extracted fields
#[derive(Debug, Clone)]
struct ParsedGrokRule {
    pub definition: String,
    pub filters: HashMap<LookupBuf, Vec<GrokFilter>>,
}

///
/// Parses alias definitions.
///
/// # Arguments
///
/// - `name` - the name of the alias
/// - `definition` - the definition of the alias
/// - `parsed_aliases` - aliases that have already been parsed
/// - `inflight_parsed_aliases` - names of the aliases that are being currently parsed(aliases can refer to other aliases) to catch circular dependencies
fn parse_alias(
    name: &str,
    definition: &str,
    aliases: &BTreeMap<String, String>,
    parsed_aliases: &mut HashMap<String, ParsedGrokRule>,
    inflight_parsed_aliases: &mut Vec<String>,
) -> Result<ParsedGrokRule, Error> {
    // track circular dependencies
    if inflight_parsed_aliases.iter().any(|a| a == name) {
        return Err(Error::CircularDependencyInAliasDefinition(
            inflight_parsed_aliases.first().unwrap().to_string(),
        ));
    } else {
        inflight_parsed_aliases.push(name.to_string());
    }

    let parsed = parse_grok_rule(definition, aliases, parsed_aliases, inflight_parsed_aliases)?;

    inflight_parsed_aliases.pop();

    Ok(parsed)
}

///
/// Parses pattern definitions.
///
/// # Arguments
///
/// - `pattern` - the definition of the pattern
/// - `parsed_aliases` - aliases that have already been parsed
/// - `grok` - an instance of Grok parser
fn parse_pattern(
    pattern: &str,
    aliases: &BTreeMap<String, String>,
    parsed_aliases: &mut HashMap<String, ParsedGrokRule>,
    grok: &mut Grok,
) -> Result<GrokRule, Error> {
    let parsed_pattern = parse_grok_rule(pattern, aliases, parsed_aliases, &mut Vec::new())?;
    let mut pattern = String::new();
    pattern.push('^');
    pattern.push_str(parsed_pattern.definition.as_str());
    pattern.push('$');

    // compile pattern
    let pattern = Arc::new(
        grok.compile(&pattern, true)
            .map_err(|e| Error::InvalidGrokExpression(pattern, e.to_string()))?,
    );

    Ok(GrokRule {
        pattern,
        filters: parsed_pattern.filters,
    })
}

/// Parses a given rule to a pure grok pattern with a set of post-processing filters.
///
/// # Arguments
///
/// - `rule` - the definition of a grok rule(can be a pattern or an alias)
/// - `aliases` - all aliases and their definitions
/// - `parsed_aliases` - aliases that have already been parsed
/// - `inflight_parsed_aliases` - names of the aliases that are being currently parsed(aliases can refer to other aliases) to catch circular dependencies
fn parse_grok_rule(
    rule: &str,
    aliases: &BTreeMap<String, String>,
    parsed_aliases: &mut HashMap<String, ParsedGrokRule>,
    inflight_parsed_aliases: &mut Vec<String>,
) -> Result<ParsedGrokRule, Error> {
    lazy_static! {
        static ref GROK_PATTERN_RE: onig::Regex =
            onig::Regex::new(r#"%\{(?:[^"\}]|(?<!\\)"(?:\\"|[^"])*(?<!\\)")+\}"#).unwrap();
    }
    // find all patterns %{}
    let raw_grok_patterns = GROK_PATTERN_RE
        .find_iter(rule)
        .map(|(start, end)| &rule[start..end])
        .collect::<Vec<&str>>();
    // parse them
    let mut grok_patterns = raw_grok_patterns
        .iter()
        .map(|pattern| {
            parse_grok_pattern(pattern)
                .map_err(|e| Error::InvalidGrokExpression(pattern.to_string(), e.to_string()))
        })
        .collect::<Result<Vec<GrokPattern>, Error>>()?;

    grok_patterns = index_repeated_fields(grok_patterns);

    let mut filters: HashMap<LookupBuf, Vec<GrokFilter>> = HashMap::new();
    let pure_grok_patterns: Vec<String> = grok_patterns
        .iter()
        .map(|pattern| {
            purify_grok_pattern(
                pattern,
                &mut filters,
                aliases,
                parsed_aliases,
                inflight_parsed_aliases,
            )
        })
        .collect::<Result<Vec<String>, Error>>()?;

    // replace grok patterns with "purified" ones
    let mut rule_definition = rule.to_string();
    for (r, pure) in raw_grok_patterns.iter().zip(pure_grok_patterns.iter()) {
        rule_definition = rule_definition.replacen(r, pure.as_str(), 1);
    }

    // collect all filters to apply later
    for pattern in &grok_patterns {
        if let GrokPattern {
            destination:
                Some(Destination {
                    filter_fn: Some(ref filter),
                    ..
                }),
            ..
        } = pattern
        {
            let dest = pattern.destination.as_ref().unwrap();
            let filter = GrokFilter::try_from(filter)?;
            filters
                .entry(dest.path.clone())
                .and_modify(|v| v.push(filter.clone()))
                .or_insert_with(|| vec![filter.clone()]);
        }
    }

    Ok(ParsedGrokRule {
        definition: rule_definition,
        filters,
    })
}

/// Replaces repeated field names with indexed versions, e.g. : field.name, field.name -> field.name.0, field.name.1 to avoid collisions in grok.
fn index_repeated_fields(grok_patterns: Vec<GrokPattern>) -> Vec<GrokPattern> {
    grok_patterns
        .iter()
        // group-by is a bit suboptimal with extra cloning, but acceptable since parsing usually happens only once
        .group_by(|pattern| pattern.destination.as_ref().map(|d| d.path.clone()))
        .into_iter()
        .flat_map(|(path, patterns)| match path {
            Some(path) => patterns
                .with_position()
                .enumerate()
                .map(|(i, pattern)| match pattern {
                    Position::First(pattern)
                    | Position::Middle(pattern)
                    | Position::Last(pattern) => {
                        let mut indexed_path = path.clone();
                        indexed_path.push_back(i as isize);
                        GrokPattern {
                            match_fn: pattern.match_fn.clone(),
                            destination: Some(Destination {
                                path: indexed_path,
                                filter_fn: pattern.destination.as_ref().unwrap().filter_fn.clone(),
                            }),
                        }
                    }
                    Position::Only(pattern) => pattern.to_owned(),
                })
                .collect::<Vec<_>>(),
            None => patterns.map(|p| p.to_owned()).collect::<Vec<_>>(),
        })
        .collect::<Vec<_>>()
}

/// Converts each rule to a pure grok rule:
///  - strips filters and collects them to apply later
///  - replaces references to aliases with their definitions
///  - replaces match functions with corresponding regex groups.
///
/// # Arguments
///
/// - `pattern` - a parsed grok pattern
/// - `filters` - post-processing filters that need to be applied after grok parsing
/// - `aliases` - all aliases and their definitions
/// - `parsed_aliases` - aliases that have already been parsed
/// - `inflight_parsed_aliases` - names of the aliases that are being currently parsed(aliases can refer to other aliases) to catch circular dependencies
fn purify_grok_pattern(
    pattern: &GrokPattern,
    mut filters: &mut HashMap<LookupBuf, Vec<GrokFilter>>,
    aliases: &BTreeMap<String, String>,
    parsed_aliases: &mut HashMap<String, ParsedGrokRule>,
    inflight_parsed_aliases: &mut Vec<String>,
) -> Result<String, Error> {
    let mut res = String::new();

    match aliases.get(pattern.match_fn.name.as_str()) {
        Some(alias_def) => {
            // this is a reference to an alias - replace it and copy all filters from the alias
            let definition = match parsed_aliases.get(pattern.match_fn.name.as_str()) {
                Some(alias) => alias.definition.clone(),
                None => {
                    // this alias is not parsed yet - let's parse it first
                    let alias = parse_alias(
                        pattern.match_fn.name.as_str(),
                        alias_def,
                        aliases,
                        parsed_aliases,
                        inflight_parsed_aliases,
                    )?;
                    parsed_aliases.insert(pattern.match_fn.name.to_string(), alias.clone());
                    alias.definition
                }
            };
            res.push_str(definition.as_str());
            parsed_aliases
                .get(pattern.match_fn.name.as_str())
                .expect("alias was not found")
                .filters
                .iter()
                .for_each(|(path, function)| {
                    filters.insert(path.to_owned(), function.to_owned());
                });
        }
        None if pattern.match_fn.name == "regex"
            || pattern.match_fn.name == "date"
            || pattern.match_fn.name == "boolean" =>
        {
            // these patterns will be converted to named capture groups e.g. (?<http.status_code>[0-9]{3})
            if let Some(destination) = &pattern.destination {
                res.push_str("(?<");
                res.push_str(destination.path.to_string().as_str());
                res.push('>');
            } else {
                res.push_str("(?:"); // non-capturing group
            }
            res.push_str(resolves_match_function(&mut filters, pattern)?.as_str());
            res.push(')');
        }
        None => {
            // these will be converted to "pure" grok patterns %{PATTERN:DESTINATION} but without filters
            res.push_str("%{");

            res.push_str(resolves_match_function(&mut filters, pattern)?.as_str());

            if let Some(destination) = &pattern.destination {
                if destination.path.is_empty() {
                    write!(res, r#":."#).unwrap(); // root
                } else {
                    write!(res, ":{}", destination.path).unwrap();
                }
            }
            res.push('}');
        }
    }
    Ok(res)
}

/// Process a match function from a given pattern:
/// - returns a grok expression(a grok pattern or a regular expression) corresponding to a given match function
/// - some match functions(e.g. number) implicitly introduce a filter to be applied to an extracted value - stores it to `filters`.
fn resolves_match_function(
    filters: &mut HashMap<LookupBuf, Vec<GrokFilter>>,
    pattern: &ast::GrokPattern,
) -> Result<String, Error> {
    let match_fn = &pattern.match_fn;
    let result = match match_fn.name.as_ref() {
        "regex" => match match_fn.args.as_ref() {
            Some(args) if !args.is_empty() => {
                if let ast::FunctionArgument::Arg(Value::Bytes(ref b)) = args[0] {
                    return Ok(String::from_utf8_lossy(b).to_string());
                }
                Err(Error::InvalidFunctionArguments(match_fn.name.clone()))
            }
            _ => Err(Error::InvalidFunctionArguments(match_fn.name.clone())),
        },
        "integer" => {
            if let Some(destination) = &pattern.destination {
                filters.insert(destination.path.clone(), vec![GrokFilter::Integer]);
            }
            Ok("integerStr".to_string())
        }
        "integerExt" => {
            if let Some(destination) = &pattern.destination {
                filters.insert(destination.path.clone(), vec![GrokFilter::IntegerExt]);
            }
            Ok("integerExtStr".to_string())
        }
        "number" => {
            if let Some(destination) = &pattern.destination {
                filters.insert(destination.path.clone(), vec![GrokFilter::Number]);
            }
            Ok("numberStr".to_string())
        }
        "numberExt" => {
            if let Some(destination) = &pattern.destination {
                filters.insert(destination.path.clone(), vec![GrokFilter::NumberExt]);
            }
            Ok("numberExtStr".to_string())
        }
        "date" => {
            return match match_fn.args.as_ref() {
                Some(args) if !args.is_empty() && args.len() <= 2 => {
                    if let ast::FunctionArgument::Arg(Value::Bytes(b)) = &args[0] {
                        let format = String::from_utf8_lossy(b);
                        let result = date::time_format_to_regex(&format, true)
                            .map_err(|_e| Error::InvalidFunctionArguments(match_fn.name.clone()))?;
                        let mut regext_opt = None;
                        if result.tz_captured {
                            regext_opt = Some(Regex::new(&result.regex).map_err(|error| {
                                error!(message = "Error compiling regex", regex = %result.regex, %error);
                                Error::InvalidFunctionArguments(match_fn.name.clone())
                            })?);
                        }
                        let strp_format = date::convert_time_format(&format).map_err(|error| {
                            error!(message = "Error compiling regex", regex = %result.regex, %error);
                            Error::InvalidFunctionArguments(match_fn.name.clone())
                        })?;
                        let mut target_tz = None;
                        if args.len() == 2 {
                            if let ast::FunctionArgument::Arg(Value::Bytes(b)) = &args[1] {
                                let tz = String::from_utf8_lossy(b);
                                date::parse_timezone(&tz).map_err(|error| {
                                    error!(message = "Invalid(unrecognized) timezone", %error);
                                    Error::InvalidFunctionArguments(match_fn.name.clone())
                                })?;
                                target_tz = Some(tz.to_string());
                            }
                        }
                        let filter = GrokFilter::Date(DateFilter {
                            original_format: format.to_string(),
                            strp_format,
                            regex_with_tz: regext_opt,
                            target_tz,
                            tz_aware: result.with_tz,
                        });
                        let result =
                            date::time_format_to_regex(&format, false).map_err(|error| {
                                error!(message = "Invalid time format", format = %format, %error);
                                Error::InvalidFunctionArguments(match_fn.name.clone())
                            })?;
                        if let Some(destination) = &pattern.destination {
                            filters.insert(destination.path.clone(), vec![filter]);
                        }
                        return Ok(result.regex);
                    }
                    Err(Error::InvalidFunctionArguments(match_fn.name.clone()))
                }
                _ => Err(Error::InvalidFunctionArguments(match_fn.name.clone())),
            };
        }
        // otherwise just add it as is, it should be a known grok pattern
        grok_pattern_name => Ok(grok_pattern_name.to_string()),
    };
    result
}

// test some tricky cases here, more high-level tests are in parse_grok
#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    #[test]
    fn supports_escaped_quotes() {
        let rules = parse_grok_rules(
            &[r#"%{notSpace:field:nullIf("with \"escaped\" quotes")}"#.to_string()],
            btreemap! {},
        )
        .expect("couldn't parse rules");
        assert!(matches!(
            &rules[0]
                .filters
                .get(&LookupBuf::from("field"))
                .expect("invalid grok pattern")[0],
            GrokFilter::NullIf(v) if *v == r#"with "escaped" quotes"#
        ));
    }
}

include!(concat!(env!("OUT_DIR"), "/patterns.rs"));
fn initialize_grok() -> Grok {
    let mut grok = grok::Grok::with_patterns();

    // Insert Datadog grok patterns.
    for &(key, value) in PATTERNS {
        grok.insert_definition(String::from(key), String::from(value));
    }
    grok
}
