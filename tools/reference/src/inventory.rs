//! Inspect declared router operations from Rust syntax without executing source.
use serde::Serialize;
use std::collections::BTreeSet;
use syn::{
    Expr, ExprMethodCall, ItemFn, Lit,
    visit::{self, Visit},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Entry {
    pub source: String,
    pub function: String,
    pub path: String,
    pub method: String,
    pub handler: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidSource,
    Syntax,
    UnsupportedRoute,
    Duplicate,
    Limit,
}
pub const MAX_ENTRIES: usize = 512;

/// Safely add input bytes under one aggregate source bound.
pub fn bounded_total(current: usize, increment: usize, maximum: usize) -> Result<usize, Error> {
    if current > maximum || increment > maximum - current {
        return Err(Error::Limit);
    }
    Ok(current + increment)
}

/// Render the bounded route inventory as a versioned, deterministic document.
pub fn document(entries: Vec<Entry>) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&serde_json::json!({
        "schema": 1,
        "version": env!("CARGO_PKG_VERSION"),
        "scope": "Explicit Axum registrations and static-service mounts. GET registers Axum HEAD dispatch too; individual handlers can reject HEAD. Configuration and authentication prerequisites require separate contract metadata.",
        "entries": entries,
    }))
    .map_err(|_| "Cannot serialize route inventory.".to_owned())
}

pub fn parse(source: &str, contents: &str) -> Result<Vec<Entry>, Error> {
    if source.is_empty()
        || source.len() > 4096
        || !source.ends_with(".rs")
        || !source.is_ascii()
        || source.chars().any(char::is_control)
        || source.contains('\\')
        || source
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(Error::InvalidSource);
    }
    if contents.len() > 1_048_576 {
        return Err(Error::Limit);
    }
    let syntax = syn::parse_file(contents).map_err(|_| Error::Syntax)?;
    let mut visitor = Inventory {
        source,
        function: String::new(),
        entries: Vec::new(),
        error: None,
    };
    visitor.visit_file(&syntax);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    merge(visitor.entries)
}

pub fn merge(entries: Vec<Entry>) -> Result<Vec<Entry>, Error> {
    if entries.len() > MAX_ENTRIES {
        return Err(Error::Limit);
    }
    let count = entries.len();
    let sorted: BTreeSet<_> = entries.into_iter().collect();
    if sorted.len() != count {
        return Err(Error::Duplicate);
    }
    Ok(sorted.into_iter().collect())
}

struct Inventory<'a> {
    source: &'a str,
    function: String,
    entries: Vec<Entry>,
    error: Option<Error>,
}
impl<'ast> Visit<'ast> for Inventory<'_> {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if self.error.is_some() {
            return;
        }
        let previous = std::mem::replace(&mut self.function, node.sig.ident.to_string());
        visit::visit_item_fn(self, node);
        self.function = previous;
    }
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if self.error.is_some() {
            return;
        }
        match node.method.to_string().as_str() {
            "route" | "route_service" | "nest_service" => {
                match entries(self.source, &self.function, node) {
                    Ok(entries) => {
                        if bounded_total(self.entries.len(), entries.len(), MAX_ENTRIES).is_err() {
                            self.error = Some(Error::Limit);
                        } else {
                            self.entries.extend(entries);
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            "nest" => self.error = Some(Error::UnsupportedRoute),
            _ => {}
        }
        if self.error.is_none() {
            visit::visit_expr_method_call(self, node);
        }
    }
}
fn entries(source: &str, function: &str, node: &ExprMethodCall) -> Result<Vec<Entry>, Error> {
    if node.args.len() != 2 || function.is_empty() {
        return Err(Error::UnsupportedRoute);
    }
    let path = match &node.args[0] {
        Expr::Lit(value) => match &value.lit {
            Lit::Str(value) => value.value(),
            _ => return Err(Error::UnsupportedRoute),
        },
        _ => return Err(Error::UnsupportedRoute),
    };
    if !path.starts_with('/') || path.len() > 512 || path.chars().any(char::is_control) {
        return Err(Error::UnsupportedRoute);
    }
    let methods = match node.method.to_string().as_str() {
        "route" => methods(&node.args[1])?,
        "route_service" => vec![("SERVICE".into(), "static-service".into())],
        "nest_service" => vec![("SERVICE_PREFIX".into(), "static-service".into())],
        _ => return Err(Error::UnsupportedRoute),
    };
    Ok(methods
        .into_iter()
        .map(|(method, handler)| Entry {
            source: source.into(),
            function: function.into(),
            path: path.clone(),
            method,
            handler,
        })
        .collect())
}
fn methods(expression: &Expr) -> Result<Vec<(String, String)>, Error> {
    match expression {
        Expr::Call(call) if call.args.len() == 1 => Ok(vec![(
            method(&identifier(&call.func)?)?,
            identifier(&call.args[0])?,
        )]),
        Expr::MethodCall(call) if call.args.len() == 1 => {
            let mut values = methods(&call.receiver)?;
            let name = call.method.to_string();
            if !matches!(name.as_str(), "layer" | "route_layer" | "with_state") {
                values.push((method(&name)?, identifier(&call.args[0])?));
            }
            Ok(values)
        }
        _ => Err(Error::UnsupportedRoute),
    }
}
fn identifier(expression: &Expr) -> Result<String, Error> {
    if let Expr::Path(path) = expression
        && let Some(segment) = path.path.segments.last()
    {
        return Ok(segment.ident.to_string());
    }
    Err(Error::UnsupportedRoute)
}
fn method(value: &str) -> Result<String, Error> {
    if [
        "get", "post", "put", "patch", "delete", "head", "options", "trace", "connect",
    ]
    .contains(&value)
    {
        Ok(value.to_ascii_uppercase())
    } else {
        Err(Error::UnsupportedRoute)
    }
}
#[cfg(test)]
#[path = "../tests/unit/inventory.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/document.rs"]
mod document_tests;
