//! Bounded inventory of the declarations in one emitted Safe obligation and
//! the definitions in its SAT model. This module does not validate model term
//! semantics, replay a value, or authorize a counterexample. It is staged under
//! `cfg(test)` until the value-pinning consumer is reviewed.

use std::collections::{BTreeMap, BTreeSet};

const MAX_INPUT_BYTES: usize = 8_000_000;
const MAX_NODES: usize = 100_000;
const MAX_DEPTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sort {
    BitVec64,
    Float64,
    String,
    ArrayBitVec64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InventoryError {
    Limit(&'static str),
    Malformed(&'static str),
    UnsupportedCommand(String),
    UnsupportedSort(String),
    DuplicateDeclaration(String),
    DuplicateModelEntry(String),
    MissingModelEntry(String),
    UnexpectedModelEntry(String),
    SortMismatch {
        name: String,
        declared: Sort,
        actual: Sort,
    },
    NonzeroDeclarationArity(String),
    NonzeroModelArity(String),
    InvalidAuxiliarySignature(String),
    MissingAuxiliary(String),
    CyclicAuxiliary(String),
    UnsupportedAuxiliaryBody(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModelValue {
    /// Semantic symbol identity; both original spellings are retained below.
    pub name: String,
    pub declaration_spelling: String,
    pub model_spelling: String,
    pub sort: Sort,
    /// Exact model bytes, not a reserialized or evaluated term.
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Auxiliary {
    pub name: String,
    pub parameter_spelling: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModelInventory {
    pub values: BTreeMap<String, ModelValue>,
    /// Only auxiliaries in the transitive closure of an array `as-array` term.
    pub auxiliaries: BTreeMap<String, Auxiliary>,
    /// Byte offset of the real top-level `(check-sat)`, not a textual match in
    /// an assertion string or comment. A later replay unit may use this split.
    pub query_prefix_end: usize,
    pub has_get_model: bool,
}

#[derive(Debug, Clone)]
struct Node<'a> {
    kind: Kind<'a>,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
enum Kind<'a> {
    Atom(&'a str),
    QuotedSymbol(&'a str),
    StringLiteral,
    List(Vec<Node<'a>>),
}

impl<'a> Node<'a> {
    fn raw<'s>(&self, source: &'s str) -> &'s str {
        &source[self.start..self.end]
    }

    fn items(&self) -> Option<&[Node<'a>]> {
        match &self.kind {
            Kind::List(items) => Some(items),
            _ => None,
        }
    }

    fn atom(&self) -> Option<&'a str> {
        match &self.kind {
            Kind::Atom(atom) => Some(*atom),
            _ => None,
        }
    }

    fn symbol(&self) -> Result<&'a str, InventoryError> {
        match &self.kind {
            Kind::Atom(atom) => Ok(*atom),
            // SMT-LIB quoted symbols denote their interior bytes. Do not
            // silently merge alternate spellings: both declaration and model
            // maps reject a repeated semantic identity, and records retain
            // each original spelling. Escaped bars are outside this bounded
            // generated fragment until their semantics are specified.
            Kind::QuotedSymbol(raw) if raw.len() > 2 && !raw.contains('\\') => {
                Ok(&raw[1..raw.len() - 1])
            }
            _ => Err(InventoryError::Malformed("symbol")),
        }
    }
}

struct Reader<'a> {
    source: &'a str,
    cursor: usize,
    nodes: usize,
}

impl<'a> Reader<'a> {
    fn document(source: &'a str) -> Result<Vec<Node<'a>>, InventoryError> {
        if source.len() > MAX_INPUT_BYTES {
            return Err(InventoryError::Limit("input bytes"));
        }
        let mut reader = Self {
            source,
            cursor: 0,
            nodes: 0,
        };
        let mut out = Vec::new();
        reader.skip_gap();
        while reader.cursor < source.len() {
            out.push(reader.node(0)?);
            reader.skip_gap();
        }
        Ok(out)
    }

    fn skip_gap(&mut self) {
        let bytes = self.source.as_bytes();
        loop {
            while bytes.get(self.cursor).is_some_and(u8::is_ascii_whitespace) {
                self.cursor += 1;
            }
            if bytes.get(self.cursor) != Some(&b';') {
                break;
            }
            while self.cursor < bytes.len() && bytes[self.cursor] != b'\n' {
                self.cursor += 1;
            }
        }
    }

    fn node(&mut self, depth: usize) -> Result<Node<'a>, InventoryError> {
        if self.nodes >= MAX_NODES {
            return Err(InventoryError::Limit("node count"));
        }
        self.nodes += 1;
        let source = self.source;
        let bytes = source.as_bytes();
        let start = self.cursor;
        let kind = match bytes.get(start) {
            Some(b'(') => {
                if depth >= MAX_DEPTH {
                    return Err(InventoryError::Limit("nesting depth"));
                }
                self.cursor += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_gap();
                    match bytes.get(self.cursor) {
                        Some(b')') => {
                            self.cursor += 1;
                            break;
                        }
                        Some(_) => items.push(self.node(depth + 1)?),
                        None => return Err(InventoryError::Malformed("unclosed list")),
                    }
                }
                Kind::List(items)
            }
            Some(b'"' | b'|') => {
                let quote = bytes[start];
                self.cursor += 1;
                loop {
                    let Some(&byte) = bytes.get(self.cursor) else {
                        return Err(InventoryError::Malformed("unclosed quote"));
                    };
                    if (quote == b'"' && byte == b'"' && bytes.get(self.cursor + 1) == Some(&b'"'))
                        || (byte == b'\\' && bytes.get(self.cursor + 1).is_some())
                    {
                        self.cursor += 2;
                    } else if byte == quote {
                        self.cursor += 1;
                        break;
                    } else {
                        self.cursor += 1;
                    }
                }
                let raw = &self.source[start..self.cursor];
                if quote == b'"' {
                    Kind::StringLiteral
                } else {
                    Kind::QuotedSymbol(raw)
                }
            }
            Some(b')' | b';') | None => {
                return Err(InventoryError::Malformed("unexpected delimiter"))
            }
            Some(_) => {
                while let Some(&byte) = bytes.get(self.cursor) {
                    if byte.is_ascii_whitespace()
                        || matches!(byte, b'(' | b')' | b';' | b'"' | b'|')
                    {
                        break;
                    }
                    self.cursor += 1;
                }
                if self.cursor == start {
                    return Err(InventoryError::Malformed("empty atom"));
                }
                Kind::Atom(&self.source[start..self.cursor])
            }
        };
        Ok(Node {
            kind,
            start,
            end: self.cursor,
        })
    }
}

fn sort_of(node: &Node<'_>, source: &str) -> Result<Sort, InventoryError> {
    if node.atom() == Some("String") {
        return Ok(Sort::String);
    }
    if let Some([underscore, bitvec, width]) = node.items() {
        if underscore.atom() == Some("_")
            && bitvec.atom() == Some("BitVec")
            && width.atom() == Some("64")
        {
            return Ok(Sort::BitVec64);
        }
    }
    if let Some([underscore, fp, exponent, significand]) = node.items() {
        if underscore.atom() == Some("_")
            && fp.atom() == Some("FloatingPoint")
            && exponent.atom() == Some("11")
            && significand.atom() == Some("53")
        {
            return Ok(Sort::Float64);
        }
    }
    if let Some([array, index, element]) = node.items() {
        if array.atom() == Some("Array")
            && sort_of(index, source) == Ok(Sort::BitVec64)
            && sort_of(element, source) == Ok(Sort::BitVec64)
        {
            return Ok(Sort::ArrayBitVec64);
        }
    }
    Err(InventoryError::UnsupportedSort(node.raw(source).into()))
}

#[derive(Debug)]
struct Declaration {
    spelling: String,
    sort: Sort,
}

fn declarations(smt: &str) -> Result<(BTreeMap<String, Declaration>, usize, bool), InventoryError> {
    let commands = Reader::document(smt)?;
    let mut found = BTreeMap::new();
    let mut check_start = None;
    let mut has_get_model = false;
    let mut saw_logic = false;
    let mut saw_body = false;
    for node in commands {
        let items = node
            .items()
            .ok_or(InventoryError::Malformed("SMT command"))?;
        let command = items
            .first()
            .and_then(Node::atom)
            .ok_or(InventoryError::Malformed("SMT command"))?;
        if check_start.is_some() {
            if command == "get-model" && items.len() == 1 && !has_get_model {
                has_get_model = true;
                continue;
            }
            return Err(InventoryError::Malformed("commands after check-sat"));
        }
        match command {
            "set-logic"
                if items.len() == 2
                    && matches!(items[1].atom(), Some("QF_BV" | "QF_FP" | "QF_S" | "QF_ABV"))
                    && !saw_logic
                    && !saw_body =>
            {
                saw_logic = true;
            }
            "assert" if items.len() == 2 => saw_body = true,
            "declare-const" if items.len() == 3 => {
                saw_body = true;
                let name = items[1].symbol()?.to_owned();
                let declaration = Declaration {
                    spelling: items[1].raw(smt).into(),
                    sort: sort_of(&items[2], smt)?,
                };
                if found.insert(name.clone(), declaration).is_some() {
                    return Err(InventoryError::DuplicateDeclaration(name));
                }
            }
            "declare-fun" if items.len() == 4 => {
                saw_body = true;
                let name = items[1].symbol()?.to_owned();
                let args = items[2]
                    .items()
                    .ok_or(InventoryError::Malformed("declaration arity"))?;
                if !args.is_empty() {
                    return Err(InventoryError::NonzeroDeclarationArity(name));
                }
                let declaration = Declaration {
                    spelling: items[1].raw(smt).into(),
                    sort: sort_of(&items[3], smt)?,
                };
                if found.insert(name.clone(), declaration).is_some() {
                    return Err(InventoryError::DuplicateDeclaration(name));
                }
            }
            "check-sat" if items.len() == 1 => check_start = Some(node.start),
            other => return Err(InventoryError::UnsupportedCommand(other.into())),
        }
    }
    let check_start = check_start.ok_or(InventoryError::Malformed("missing check-sat"))?;
    if !saw_logic {
        return Err(InventoryError::Malformed("missing set-logic"));
    }
    if !has_get_model {
        return Err(InventoryError::Malformed("missing get-model"));
    }
    Ok((found, check_start, has_get_model))
}

#[derive(Debug, Clone)]
struct Definition<'a> {
    spelling: String,
    quoted_name: bool,
    parameters: Vec<Formal>,
    sort: Sort,
    body: Node<'a>,
}

#[derive(Debug, Clone)]
struct Formal {
    spelling: String,
    sort: Sort,
}

fn definitions(model: &str) -> Result<BTreeMap<String, Definition<'_>>, InventoryError> {
    let response = Reader::document(model)?;
    let [verdict, model_list] = response.as_slice() else {
        return Err(InventoryError::Malformed("SAT model response cardinality"));
    };
    if verdict.atom() != Some("sat") {
        return Err(InventoryError::Malformed("SAT verdict"));
    }
    let entries = model_list
        .items()
        .ok_or(InventoryError::Malformed("model list"))?;
    let mut found = BTreeMap::new();
    for entry in entries {
        let [keyword, name, parameters, sort, body] = entry
            .items()
            .ok_or(InventoryError::Malformed("define-fun"))?
        else {
            return Err(InventoryError::Malformed("define-fun components"));
        };
        if keyword.atom() != Some("define-fun") {
            return Err(InventoryError::Malformed("model entry command"));
        }
        let identity = name.symbol()?.to_owned();
        let params = parameters
            .items()
            .ok_or(InventoryError::Malformed("model formals"))?;
        let mut parsed_params = Vec::new();
        let mut parameter_names = BTreeSet::new();
        for parameter in params {
            let [param_name, param_sort] = parameter
                .items()
                .ok_or(InventoryError::Malformed("model formal"))?
            else {
                return Err(InventoryError::Malformed("model formal pair"));
            };
            let param_spelling = param_name.raw(model).to_owned();
            let param_identity = param_name.symbol()?.to_owned();
            if !parameter_names.insert(param_identity.clone()) {
                return Err(InventoryError::Malformed("duplicate model formal"));
            }
            parsed_params.push(Formal {
                spelling: param_spelling,
                sort: sort_of(param_sort, model)?,
            });
        }
        let definition = Definition {
            spelling: name.raw(model).into(),
            quoted_name: matches!(&name.kind, Kind::QuotedSymbol(_)),
            parameters: parsed_params,
            sort: sort_of(sort, model)?,
            body: body.clone(),
        };
        if found.insert(identity.clone(), definition).is_some() {
            return Err(InventoryError::DuplicateModelEntry(identity));
        }
    }
    Ok(found)
}

fn as_array_refs(node: &Node<'_>, out: &mut BTreeSet<String>) -> Result<(), InventoryError> {
    let Some(items) = node.items() else {
        return Ok(());
    };
    if items.first().and_then(Node::atom) == Some("_")
        && items.get(1).and_then(Node::atom) == Some("as-array")
    {
        let [_, _, target] = items else {
            return Err(InventoryError::Malformed("as-array reference"));
        };
        if target
            .atom()
            .is_some_and(|atom| !is_generated_helper_symbol(atom))
        {
            return Err(InventoryError::Malformed("unsupported as-array symbol"));
        }
        out.insert(target.symbol()?.into());
        return Ok(());
    }
    for item in items {
        as_array_refs(item, out)?;
    }
    Ok(())
}

fn is_bv_literal(atom: &str) -> bool {
    if let Some(digits) = atom.strip_prefix("#x") {
        return !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    if let Some(digits) = atom.strip_prefix("#b") {
        return !digits.is_empty() && digits.bytes().all(|byte| matches!(byte, b'0' | b'1'));
    }
    false
}

fn is_supported_helper_operator(name: &str) -> bool {
    matches!(
        name,
        "ite"
            | "="
            | "distinct"
            | "and"
            | "or"
            | "not"
            | "bvnot"
            | "bvneg"
            | "bvand"
            | "bvor"
            | "bvxor"
            | "bvadd"
            | "bvsub"
            | "bvmul"
            | "bvudiv"
            | "bvurem"
            | "bvshl"
            | "bvlshr"
            | "bvashr"
            | "concat"
    )
}

/// Z3's generated array helpers in this bounded fragment use `k!` plus
/// decimal digits. Other unquoted heads could be SMT operators outside the
/// small allowlist and must not inherit a quoted helper's semantic identity.
/// A different source-observed helper spelling requires a reviewed extension.
fn is_generated_helper_symbol(name: &str) -> bool {
    name.strip_prefix("k!").is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Only names in this bounded helper-expression grammar may appear in a
/// helper body. In particular, an undefined `(k!1 x)` cannot disappear from
/// the dependency graph merely because `k!1` was absent from the model.
/// This is still only reference inventory, not term typing or evaluation.
fn helper_dependencies(
    node: &Node<'_>,
    parameters: &BTreeSet<String>,
    helpers: &BTreeMap<String, Definition<'_>>,
    out: &mut BTreeSet<String>,
) -> Result<(), InventoryError> {
    match &node.kind {
        Kind::Atom(atom) => {
            if !is_supported_helper_operator(atom)
                && (parameters.contains(*atom) || is_bv_literal(atom))
            {
                Ok(())
            } else {
                Err(InventoryError::UnsupportedAuxiliaryBody((*atom).into()))
            }
        }
        Kind::QuotedSymbol(raw) => {
            if parameters.contains(*raw) {
                Ok(())
            } else {
                Err(InventoryError::UnsupportedAuxiliaryBody(
                    node.symbol()?.into(),
                ))
            }
        }
        Kind::StringLiteral => Err(InventoryError::UnsupportedAuxiliaryBody(
            "string literal".into(),
        )),
        Kind::List(items) => {
            if let [underscore, literal, width] = items.as_slice() {
                if underscore.atom() == Some("_")
                    && width.atom() == Some("64")
                    && literal.atom().is_some_and(|atom| {
                        atom.strip_prefix("bv").is_some_and(|digits| {
                            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                        })
                    })
                {
                    return Ok(());
                }
            }
            let Some(head) = items.first() else {
                return Err(InventoryError::UnsupportedAuxiliaryBody(
                    "empty application".into(),
                ));
            };
            // An unquoted `ite`/`bvadd` is a built-in operator. A quoted
            // `|ite|`/`|bvadd|` is a user symbol and must have a definition.
            // Semantic symbol equality is only appropriate after deciding
            // which syntactic category the head belongs to.
            if !head.atom().is_some_and(is_supported_helper_operator) {
                let name = head.symbol().map_err(|_| {
                    InventoryError::UnsupportedAuxiliaryBody("compound application".into())
                })?;
                if head
                    .atom()
                    .is_some_and(|atom| !is_generated_helper_symbol(atom))
                {
                    return Err(InventoryError::UnsupportedAuxiliaryBody(name.into()));
                }
                if helpers.contains_key(name) {
                    out.insert(name.into());
                } else {
                    return Err(InventoryError::UnsupportedAuxiliaryBody(name.into()));
                }
            }
            // The later term checker owns built-in arity and sort validation.
            for argument in &items[1..] {
                helper_dependencies(argument, parameters, helpers, out)?;
            }
            Ok(())
        }
    }
}

fn visit_helper(
    name: &str,
    helpers: &BTreeMap<String, Definition<'_>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    depth: usize,
) -> Result<(), InventoryError> {
    if depth >= MAX_DEPTH {
        return Err(InventoryError::Limit("auxiliary dependency depth"));
    }
    if visited.contains(name) {
        return Ok(());
    }
    let definition = helpers
        .get(name)
        .ok_or_else(|| InventoryError::MissingAuxiliary(name.into()))?;
    if !visiting.insert(name.into()) {
        return Err(InventoryError::CyclicAuxiliary(name.into()));
    }
    if definition.parameters.len() != 1
        || definition.parameters[0].sort != Sort::BitVec64
        || definition.sort != Sort::BitVec64
        || (!definition.quoted_name && !is_generated_helper_symbol(name))
    {
        return Err(InventoryError::InvalidAuxiliarySignature(name.into()));
    }
    let mut dependencies = BTreeSet::new();
    let parameters = definition
        .parameters
        .iter()
        // Keep the lexical category of a formal reference. An unquoted
        // reserved token cannot silently denote a quoted formal with the
        // same interior text; ordinary alternate spellings may be supported
        // later with a complete SMT symbol grammar.
        .map(|formal| formal.spelling.clone())
        .collect();
    helper_dependencies(&definition.body, &parameters, helpers, &mut dependencies)?;
    for dependency in dependencies {
        visit_helper(&dependency, helpers, visiting, visited, depth + 1)?;
    }
    visiting.remove(name);
    visited.insert(name.into());
    Ok(())
}

/// Compare exactly the declared symbols of one emitted query with one SAT
/// model. Sort and arity are checked structurally, and unexpected definitions
/// are accepted only when needed by a validated `as-array` helper closure.
/// Returned bodies are raw bytes for a later value-pinning stage, not evidence
/// that the terms have the declared sorts or denote source-level values.
pub(super) fn inventory(smt: &str, model: &str) -> Result<ModelInventory, InventoryError> {
    let (declarations, query_prefix_end, has_get_model) = declarations(smt)?;
    let definitions = definitions(model)?;
    let mut values = BTreeMap::new();
    let mut helpers = BTreeMap::new();
    for (name, definition) in &definitions {
        if let Some(declaration) = declarations.get(name) {
            if !definition.parameters.is_empty() {
                return Err(InventoryError::NonzeroModelArity(name.clone()));
            }
            if definition.sort != declaration.sort {
                return Err(InventoryError::SortMismatch {
                    name: name.clone(),
                    declared: declaration.sort,
                    actual: definition.sort,
                });
            }
            values.insert(
                name.clone(),
                ModelValue {
                    name: name.clone(),
                    declaration_spelling: declaration.spelling.clone(),
                    model_spelling: definition.spelling.clone(),
                    sort: definition.sort,
                    body: definition.body.raw(model).into(),
                },
            );
        } else {
            helpers.insert(name.clone(), definition.clone());
        }
    }
    for name in declarations.keys() {
        if !values.contains_key(name) {
            return Err(InventoryError::MissingModelEntry(name.clone()));
        }
    }
    let mut required = BTreeSet::new();
    // Inspect the original parsed bodies, rather than searching raw model
    // text: an `as-array` spelling inside a String cannot name an auxiliary.
    for (name, value) in &values {
        if value.sort == Sort::ArrayBitVec64 {
            let body = &definitions
                .get(name)
                .ok_or(InventoryError::Malformed("model body inventory"))?
                .body;
            as_array_refs(body, &mut required)?;
        }
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for name in required {
        visit_helper(&name, &helpers, &mut visiting, &mut visited, 0)?;
    }
    for name in helpers.keys() {
        if !visited.contains(name) {
            return Err(InventoryError::UnexpectedModelEntry(name.clone()));
        }
    }
    let auxiliaries = helpers
        .into_iter()
        .map(|(name, definition)| {
            let parameter_spelling = definition.parameters[0].spelling.clone();
            let body = definition.body.raw(model).into();
            (
                name.clone(),
                Auxiliary {
                    name,
                    parameter_spelling,
                    body,
                },
            )
        })
        .collect();
    Ok(ModelInventory {
        values,
        auxiliaries,
        query_prefix_end,
        has_get_model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BV: &str = "(_ BitVec 64)";
    const FP: &str = "(_ FloatingPoint 11 53)";
    const ARR: &str = "(Array (_ BitVec 64) (_ BitVec 64))";

    fn query(logic: &str, declaration: &str) -> String {
        format!("(set-logic {logic})\n{declaration}\n(assert true)\n(check-sat)\n(get-model)\n")
    }

    #[test]
    fn direct_safe_bv_model_has_one_exact_value_and_pairwise_refusals() {
        let smt = query("QF_BV", &format!("(declare-const anb_x {BV})"));
        let good = format!("sat\n((define-fun anb_x () {BV} #x0000000000000001))\n");
        let checked = inventory(&smt, &good).unwrap();
        assert_eq!(checked.values["anb_x"].name, "anb_x");
        assert_eq!(checked.values["anb_x"].sort, Sort::BitVec64);
        assert_eq!(checked.values["anb_x"].body, "#x0000000000000001");
        assert!(checked.has_get_model);
        assert_eq!(
            &smt[checked.query_prefix_end..],
            "(check-sat)\n(get-model)\n"
        );
        assert_eq!(
            inventory(&smt, "sat\n()"),
            Err(InventoryError::MissingModelEntry("anb_x".into()))
        );
        assert_eq!(
            inventory(&smt, "sat\n((define-fun anb_x () String \"x\"))"),
            Err(InventoryError::SortMismatch {
                name: "anb_x".into(),
                declared: Sort::BitVec64,
                actual: Sort::String
            })
        );
        assert_eq!(
            inventory(
                &smt,
                &format!("sat\n((define-fun anb_x ((y {BV})) {BV} y))")
            ),
            Err(InventoryError::NonzeroModelArity("anb_x".into()))
        );
        assert_eq!(
            inventory(
                &smt,
                &format!("sat\n((define-fun anb_x () {BV} #x1) (define-fun anb_x () {BV} #x2))")
            ),
            Err(InventoryError::DuplicateModelEntry("anb_x".into()))
        );
        assert_eq!(
            inventory(
                &smt,
                &format!(
                    "sat\n((define-fun anb_x () {BV} #x1) (define-fun unrelated () {BV} #x2))"
                )
            ),
            Err(InventoryError::UnexpectedModelEntry("unrelated".into()))
        );
    }

    #[test]
    fn direct_safe_fp_and_string_models_each_require_their_own_sort_and_value() {
        let fp_smt = query("QF_FP", &format!("(declare-const anb_f {FP})"));
        let fp_good = format!("sat\n((define-fun anb_f () {FP} (_ +zero 11 53)))");
        assert_eq!(
            inventory(&fp_smt, &fp_good).unwrap().values["anb_f"].sort,
            Sort::Float64
        );
        assert_eq!(
            inventory(&fp_smt, "sat\n()"),
            Err(InventoryError::MissingModelEntry("anb_f".into()))
        );
        assert_eq!(
            inventory(&fp_smt, "sat\n((define-fun anb_f () String \"0\"))"),
            Err(InventoryError::SortMismatch {
                name: "anb_f".into(),
                declared: Sort::Float64,
                actual: Sort::String
            })
        );
        let string_smt = query("QF_S", "(declare-const anb_s String)");
        let string_good = "sat\n((define-fun anb_s () String \"a\"\"b\"))";
        assert_eq!(
            inventory(&string_smt, string_good).unwrap().values["anb_s"].body,
            "\"a\"\"b\""
        );
        assert_eq!(
            inventory(&string_smt, &fp_good),
            Err(InventoryError::MissingModelEntry("anb_s".into()))
        );
        assert_eq!(
            inventory(&string_smt, "sat\n()"),
            Err(InventoryError::MissingModelEntry("anb_s".into()))
        );
        assert_eq!(
            inventory(
                &string_smt,
                "sat\n((define-fun anb_s () String \"(_ as-array absent)\"))"
            )
            .unwrap()
            .values["anb_s"]
                .body,
            "\"(_ as-array absent)\""
        );
        assert_eq!(
            inventory(
                &string_smt,
                "sat\n((define-fun anb_s () (_ BitVec 64) #x0))"
            ),
            Err(InventoryError::SortMismatch {
                name: "anb_s".into(),
                declared: Sort::String,
                actual: Sort::BitVec64
            })
        );
    }

    #[test]
    fn direct_safe_array_const_store_and_as_array_inventory_are_bounded() {
        let smt = query("QF_ABV", &format!("(declare-fun anb_a () {ARR})"));
        let constant =
            format!("sat\n((define-fun anb_a () {ARR} ((as const {ARR}) #x0000000000000000)))");
        assert_eq!(
            inventory(&smt, &constant).unwrap().values["anb_a"].sort,
            Sort::ArrayBitVec64
        );
        let store =
            format!("sat\n((define-fun anb_a () {ARR} (store ((as const {ARR}) #x0) #x1 #x2)))");
        assert!(inventory(&smt, &store).is_ok());
        let auxiliary = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} x))");
        let checked = inventory(&smt, &auxiliary).unwrap();
        assert_eq!(checked.auxiliaries["k!0"].parameter_spelling, "x");
        assert_eq!(checked.auxiliaries["k!0"].name, "k!0");
        assert_eq!(checked.auxiliaries["k!0"].body, "x");
        let conditional_helper = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} (ite (= x #x0000000000000000) #x0000000000000001 #x0000000000000002)))");
        assert!(inventory(&smt, &conditional_helper).is_ok());
        assert_eq!(
            inventory(&smt, "sat\n()"),
            Err(InventoryError::MissingModelEntry("anb_a".into()))
        );
        assert_eq!(
            inventory(&smt, &format!("sat\n((define-fun anb_a () {BV} #x0))")),
            Err(InventoryError::SortMismatch {
                name: "anb_a".into(),
                declared: Sort::ArrayBitVec64,
                actual: Sort::BitVec64
            })
        );
        assert_eq!(
            inventory(
                &smt,
                &format!("sat\n((define-fun anb_a () {ARR} (_ as-array absent)))")
            ),
            Err(InventoryError::Malformed("unsupported as-array symbol"))
        );
        assert_eq!(
            inventory(
                &smt,
                &format!("sat\n((define-fun anb_a () {ARR} (_ as-array |absent|)))")
            ),
            Err(InventoryError::MissingAuxiliary("absent".into()))
        );
        assert_eq!(inventory(&smt, &format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 () {BV} #x0))")), Err(InventoryError::InvalidAuxiliarySignature("k!0".into())));
    }

    #[test]
    fn auxiliary_must_be_used_once_and_acyclic() {
        let smt = query("QF_ABV", &format!("(declare-const anb_a {ARR})"));
        let extra = format!("sat\n((define-fun anb_a () {ARR} ((as const {ARR}) #x0)) (define-fun k!0 ((x {BV})) {BV} x))");
        assert_eq!(
            inventory(&smt, &extra),
            Err(InventoryError::UnexpectedModelEntry("k!0".into()))
        );
        let cycle = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} (k!1 x)) (define-fun k!1 ((y {BV})) {BV} (k!0 y)))");
        assert!(matches!(
            inventory(&smt, &cycle),
            Err(InventoryError::CyclicAuxiliary(_))
        ));
        let missing_dependency = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} (k!1 x)))");
        assert_eq!(
            inventory(&smt, &missing_dependency),
            Err(InventoryError::UnsupportedAuxiliaryBody("k!1".into()))
        );
        let quoted_builtin_missing = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} (|ite| x)))");
        assert_eq!(
            inventory(&smt, &quoted_builtin_missing),
            Err(InventoryError::UnsupportedAuxiliaryBody("ite".into()))
        );
        let quoted_builtin_defined = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} (|ite| x)) (define-fun |ite| ((y {BV})) {BV} y))");
        assert!(inventory(&smt, &quoted_builtin_defined).is_ok());
        let bare_builtin_does_not_use_quoted_helper = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} (ite x)) (define-fun |ite| ((y {BV})) {BV} y))");
        assert_eq!(
            inventory(&smt, &bare_builtin_does_not_use_quoted_helper),
            Err(InventoryError::UnexpectedModelEntry("ite".into()))
        );
        let quoted_builtin_as_array = format!("sat\n((define-fun anb_a () {ARR} (_ as-array |ite|)) (define-fun |ite| ((x {BV})) {BV} x))");
        assert!(inventory(&smt, &quoted_builtin_as_array).is_ok());
        let bare_builtin_as_array = format!("sat\n((define-fun anb_a () {ARR} (_ as-array ite)) (define-fun |ite| ((x {BV})) {BV} x))");
        assert_eq!(
            inventory(&smt, &bare_builtin_as_array),
            Err(InventoryError::Malformed("unsupported as-array symbol"))
        );
        let unsupported_unquoted_operator = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} (bvult x x)) (define-fun |bvult| ((y {BV})) {BV} y))");
        assert_eq!(
            inventory(&smt, &unsupported_unquoted_operator),
            Err(InventoryError::UnsupportedAuxiliaryBody("bvult".into()))
        );
        let quoted_nonbuiltin_helper = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} (|bvult| x)) (define-fun |bvult| ((y {BV})) {BV} y))");
        assert!(inventory(&smt, &quoted_nonbuiltin_helper).is_ok());
        let quoted_formal = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((|ite| {BV})) {BV} |ite|))");
        assert!(inventory(&smt, &quoted_formal).is_ok());
        let bare_reserved_formal = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((|ite| {BV})) {BV} ite))");
        assert_eq!(
            inventory(&smt, &bare_reserved_formal),
            Err(InventoryError::UnsupportedAuxiliaryBody("ite".into()))
        );
        let alternate_formal_spelling = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((|x| {BV})) {BV} x))");
        assert_eq!(
            inventory(&smt, &alternate_formal_spelling),
            Err(InventoryError::UnsupportedAuxiliaryBody("x".into()))
        );
        let duplicate = format!("sat\n((define-fun anb_a () {ARR} (_ as-array k!0)) (define-fun k!0 ((x {BV})) {BV} x) (define-fun |k!0| ((x {BV})) {BV} x))");
        assert_eq!(
            inventory(&smt, &duplicate),
            Err(InventoryError::DuplicateModelEntry("k!0".into()))
        );
    }

    #[test]
    fn ground_model_and_command_boundary_ignore_quoted_text_and_comments() {
        let smt = "(set-logic QF_S)\n; (check-sat) inside comment\n(assert (= \"(check-sat)\" \"(check-sat)\"))\n(check-sat)\n(get-model)\n";
        let checked = inventory(smt, "sat\n(\n)\n").unwrap();
        assert!(checked.values.is_empty());
        assert_eq!(
            &smt[checked.query_prefix_end..],
            "(check-sat)\n(get-model)\n"
        );
        assert_eq!(
            inventory(smt, "sat"),
            Err(InventoryError::Malformed("SAT model response cardinality"))
        );
        assert_eq!(
            inventory(smt, "sat\n()\nunsat"),
            Err(InventoryError::Malformed("SAT model response cardinality"))
        );
        assert_eq!(
            inventory(smt, "\"sat\"\n()"),
            Err(InventoryError::Malformed("SAT verdict"))
        );
        assert_eq!(
            inventory("(set-logic QF_BV)\n(check-sat)", "sat\n()"),
            Err(InventoryError::Malformed("missing get-model"))
        );
        assert_eq!(
            inventory("(check-sat)\n(get-model)", "sat\n()"),
            Err(InventoryError::Malformed("missing set-logic"))
        );
    }

    #[test]
    fn mixed_safe_declarations_do_not_let_one_binding_stand_for_another() {
        let smt = format!(
            "(set-logic QF_ABV)\n(declare-const anb_x {BV})\n(declare-const anb_y {BV})\n(declare-const anb_a {ARR})\n(assert true)\n(check-sat)\n(get-model)\n"
        );
        let array_body = format!("((as const {ARR}) #x0)");
        let good = format!(
            "sat\n((define-fun anb_x () {BV} #x1) (define-fun anb_y () {BV} #x2) (define-fun anb_a () {ARR} {array_body}))"
        );
        assert_eq!(
            inventory(&smt, &good)
                .unwrap()
                .values
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["anb_a", "anb_x", "anb_y"]
        );
        let missing = format!(
            "sat\n((define-fun anb_x () {BV} #x1) (define-fun anb_a () {ARR} {array_body}))"
        );
        assert_eq!(
            inventory(&smt, &missing),
            Err(InventoryError::MissingModelEntry("anb_y".into()))
        );
        let duplicate = format!(
            "sat\n((define-fun anb_x () {BV} #x1) (define-fun anb_x () {BV} #x2) (define-fun anb_y () {BV} #x2) (define-fun anb_a () {ARR} {array_body}))"
        );
        assert_eq!(
            inventory(&smt, &duplicate),
            Err(InventoryError::DuplicateModelEntry("anb_x".into()))
        );
    }

    #[test]
    fn semantic_symbol_collisions_refuse_without_discarding_spelling() {
        let smt = query("QF_BV", &format!("(declare-const |anb x| {BV})"));
        let model = format!("sat\n((define-fun |anb x| () {BV} #x0))");
        let checked = inventory(&smt, &model).unwrap();
        assert_eq!(checked.values["anb x"].declaration_spelling, "|anb x|");
        assert_eq!(checked.values["anb x"].model_spelling, "|anb x|");
        let duplicate_decl =
            format!("(declare-const x {BV})\n(declare-const |x| {BV})\n(check-sat)");
        assert_eq!(
            inventory(&duplicate_decl, "sat\n()"),
            Err(InventoryError::DuplicateDeclaration("x".into()))
        );
        let bare_smt = query("QF_BV", &format!("(declare-const x {BV})"));
        let alias_model = format!("sat\n((define-fun x () {BV} #x0) (define-fun |x| () {BV} #x1))");
        assert_eq!(
            inventory(&bare_smt, &alias_model),
            Err(InventoryError::DuplicateModelEntry("x".into()))
        );
    }

    #[test]
    fn unsupported_or_unscoped_declarations_fail_before_any_match() {
        let nonzero = format!("(declare-fun f ({BV}) {BV})\n(check-sat)");
        assert_eq!(
            inventory(&nonzero, "sat\n()"),
            Err(InventoryError::NonzeroDeclarationArity("f".into()))
        );
        let reset = "(declare-const x (_ BitVec 64))\n(reset)\n(check-sat)";
        assert_eq!(
            inventory(reset, "sat\n()"),
            Err(InventoryError::UnsupportedCommand("reset".into()))
        );
        let unsupported_sort = "(declare-const x Int)\n(check-sat)";
        assert_eq!(
            inventory(unsupported_sort, "sat\n()"),
            Err(InventoryError::UnsupportedSort("Int".into()))
        );
    }
}
