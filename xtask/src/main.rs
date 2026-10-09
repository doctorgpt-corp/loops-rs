//! Repository tasks. Run with `cargo xtask <command>`.
//!
//! `normalize-spec <in.yaml> <out.json>` rewrites the Loops OpenAPI spec into
//! a form oas3-gen generates correct Rust for. The shipped `src/openapi.yaml`
//! is never modified; only the generator's input is.
//!
//! Two patterns in the upstream spec trip the generator:
//!
//! 1. A `$ref` to a component schema whose `type` includes `"null"` (e.g.
//!    `AudienceFilter`). oas3-gen generates the referencing field as
//!    non-optional, so a `null` in a response fails to decode. Each such
//!    reference becomes `anyOf: [{$ref}, {type: "null"}]`, which generates
//!    `Option<T>`.
//! 2. An inline property `enum` whose `type` includes `"null"`. oas3-gen
//!    ignores the null type and turns a `"null"` enum entry into a variant
//!    matching the *string* `"null"`. Each such enum is hoisted into a named
//!    component schema without the null entry and referenced through
//!    `anyOf: [{$ref}, {type: "null"}]`, which generates `Option<Enum>`.
//!
//! A third pattern decodes silently wrong: a `oneOf` with a `discriminator`
//! but no `mapping`. oas3-gen emits an untagged enum whose first variant
//! accepts every member, so a workflow `ExitAction` node decodes as
//! `SignupTrigger`. Each such discriminator gets the explicit `mapping` the
//! members imply (each member's single-value enum for the discriminator
//! property), which generates a tagged enum.
//!
//! Finally, `additionalProperties: false` is dropped everywhere. oas3-gen
//! turns it into `#[serde(deny_unknown_fields)]`, which makes a response fail
//! to decode as soon as Loops adds a field, and makes a discriminated member
//! reject its own tag (the generator skips deserializing the tag field). Only
//! decoding is affected: request bodies are serialized, never deserialized.

use std::collections::BTreeSet;
use std::process::ExitCode;

use serde_json::{Map, Value, json};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ["normalize-spec", input, output] = args.iter().map(String::as_str).collect::<Vec<_>>()[..]
    else {
        eprintln!("usage: cargo xtask normalize-spec <in.yaml> <out.json>");
        return ExitCode::FAILURE;
    };
    match normalize_file(input, output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("normalize-spec: {err}");
            ExitCode::FAILURE
        }
    }
}

fn normalize_file(input: &str, output: &str) -> Result<(), String> {
    let source = std::fs::read_to_string(input).map_err(|err| format!("reading {input}: {err}"))?;
    let mut spec: Value =
        serde_yaml_ng::from_str(&source).map_err(|err| format!("parsing {input}: {err}"))?;
    let report = normalize(&mut spec)?;
    for line in report {
        eprintln!("normalize-spec: {line}");
    }
    let rendered = serde_json::to_string_pretty(&spec).map_err(|err| err.to_string())?;
    std::fs::write(output, rendered + "\n").map_err(|err| format!("writing {output}: {err}"))
}

/// Apply both rewrites in place and describe what changed.
fn normalize(spec: &mut Value) -> Result<Vec<String>, String> {
    let mut report = Vec::new();

    let existing: BTreeSet<String> = schemas(spec)?.keys().cloned().collect();
    let mut hoisted = Map::new();
    let mut hoisted_names = Vec::new();
    for schema in schemas(spec)?.values_mut() {
        hoist_nullable_enums(schema, &existing, &mut hoisted, &mut hoisted_names)?;
    }
    if let Some(paths) = spec.get_mut("paths") {
        hoist_nullable_enums(paths, &existing, &mut hoisted, &mut hoisted_names)?;
    }
    schemas(spec)?.extend(hoisted);
    if !hoisted_names.is_empty() {
        report.push(format!(
            "hoisted nullable enums: {}",
            hoisted_names.join(", ")
        ));
    }

    let nullable: BTreeSet<String> = schemas(spec)?
        .iter()
        .filter(|(_, schema)| has_null_type(schema))
        .map(|(name, _)| name.clone())
        .collect();
    let mut rewritten = 0;
    wrap_nullable_refs(spec, &nullable, &mut rewritten);
    if rewritten > 0 {
        report.push(format!(
            "made {rewritten} references to nullable schemas optional ({})",
            nullable.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }

    let mut closed = 0;
    open_closed_objects(spec, &mut closed);
    if closed > 0 {
        report.push(format!(
            "dropped additionalProperties: false from {closed} schemas"
        ));
    }

    let mapped = add_discriminator_mappings(spec)?;
    if !mapped.is_empty() {
        report.push(format!(
            "derived discriminator mappings: {}",
            mapped.join(", ")
        ));
    }
    Ok(report)
}

/// Remove every `additionalProperties: false`.
fn open_closed_objects(node: &mut Value, count: &mut usize) {
    match node {
        Value::Object(object) => {
            if object.get("additionalProperties") == Some(&Value::Bool(false)) {
                object.remove("additionalProperties");
                *count += 1;
            }
            for value in object.values_mut() {
                open_closed_objects(value, count);
            }
        }
        Value::Array(items) => {
            for item in items {
                open_closed_objects(item, count);
            }
        }
        _ => {}
    }
}

/// Give every `oneOf` + `discriminator` without a `mapping` an explicit one.
/// Fails when a member does not pin the discriminator property to exactly one
/// value, since the union could then not be decoded reliably.
fn add_discriminator_mappings(spec: &mut Value) -> Result<Vec<String>, String> {
    let all = schemas(spec)?.clone();
    let mut mapped = Vec::new();
    for (name, schema) in schemas(spec)? {
        let Some(property) = schema
            .pointer("/discriminator/propertyName")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        if schema.pointer("/discriminator/mapping").is_some() {
            continue;
        }
        let Some(members) = schema.get("oneOf").and_then(Value::as_array) else {
            continue;
        };
        let mut mapping = Map::new();
        for member in members {
            let reference = member
                .get("$ref")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("`{name}`: discriminated oneOf member is not a $ref"))?;
            let target = reference.rsplit('/').next().unwrap_or_default();
            let tag = all
                .get(target)
                .and_then(|member| member.pointer(&format!("/properties/{property}/enum")))
                .and_then(Value::as_array)
                .and_then(|values| match values.as_slice() {
                    [Value::String(tag)] => Some(tag.clone()),
                    _ => None,
                })
                .ok_or_else(|| {
                    format!(
                        "`{name}`: member `{target}` does not pin `{property}` to a single string"
                    )
                })?;
            if mapping
                .insert(tag.clone(), Value::from(reference))
                .is_some()
            {
                return Err(format!(
                    "`{name}`: two members share the `{property}` value `{tag}`"
                ));
            }
        }
        schema["discriminator"]["mapping"] = Value::Object(mapping);
        mapped.push(name.clone());
    }
    Ok(mapped)
}

fn schemas(spec: &mut Value) -> Result<&mut Map<String, Value>, String> {
    spec.pointer_mut("/components/schemas")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "spec has no components.schemas".to_owned())
}

fn has_null_type(schema: &Value) -> bool {
    schema
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|types| types.iter().any(|t| t == "null"))
}

fn nullable_ref(reference: &str, rest: Map<String, Value>) -> Value {
    let mut wrapped = Map::new();
    wrapped.insert(
        "anyOf".to_owned(),
        json!([{ "$ref": reference }, { "type": "null" }]),
    );
    wrapped.extend(rest);
    Value::Object(wrapped)
}

/// Rewrite every property schema that is a nullable inline enum.
fn hoist_nullable_enums(
    node: &mut Value,
    existing: &BTreeSet<String>,
    hoisted: &mut Map<String, Value>,
    names: &mut Vec<String>,
) -> Result<(), String> {
    match node {
        Value::Object(object) => {
            if let Some(Value::Object(properties)) = object.get_mut("properties") {
                for (property, schema) in properties.iter_mut() {
                    if schema.get("enum").is_some() && has_null_type(schema) {
                        let name = hoist(property, schema, existing, hoisted)?;
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
            }
            for value in object.values_mut() {
                hoist_nullable_enums(value, existing, hoisted, names)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                hoist_nullable_enums(item, existing, hoisted, names)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn hoist(
    property: &str,
    schema: &mut Value,
    existing: &BTreeSet<String>,
    hoisted: &mut Map<String, Value>,
) -> Result<String, String> {
    let Value::Object(object) = std::mem::take(schema) else {
        unreachable!("only objects carry an enum");
    };
    let mut definition = Map::new();
    let mut rest = Map::new();
    for (key, value) in object {
        match key.as_str() {
            "type" => {
                let types: Vec<Value> = value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|t| *t != "null")
                    .cloned()
                    .collect();
                let single = match types.as_slice() {
                    [only] => only.clone(),
                    _ => Value::Array(types),
                };
                definition.insert(key, single);
            }
            "enum" => {
                let values: Vec<Value> = value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|v| !v.is_null() && *v != "null")
                    .cloned()
                    .collect();
                definition.insert(key, Value::Array(values));
            }
            _ => {
                rest.insert(key, value);
            }
        }
    }
    let definition = Value::Object(definition);
    let base = pascal_case(property);
    let mut name = base.clone();
    for suffix in 2.. {
        if existing.contains(&name) {
            return Err(format!(
                "cannot hoist the nullable enum `{property}`: component schema `{name}` already exists"
            ));
        }
        match hoisted.get(&name) {
            None => {
                hoisted.insert(name.clone(), definition);
                break;
            }
            Some(same) if *same == definition => break,
            Some(_) => name = format!("{base}{suffix}"),
        }
    }
    *schema = nullable_ref(&format!("#/components/schemas/{name}"), rest);
    Ok(name)
}

/// Wrap every `{"$ref": <nullable schema>}` map value in an `anyOf` with null.
fn wrap_nullable_refs(node: &mut Value, nullable: &BTreeSet<String>, rewritten: &mut usize) {
    match node {
        Value::Object(object) => {
            for value in object.values_mut() {
                if let Some(reference) = value.get("$ref").and_then(Value::as_str) {
                    let name = reference.rsplit('/').next().unwrap_or_default();
                    if nullable.contains(name) {
                        let reference = reference.to_owned();
                        let Value::Object(mut fields) = std::mem::take(value) else {
                            unreachable!("a $ref lives in an object");
                        };
                        fields.remove("$ref");
                        *value = nullable_ref(&reference, fields);
                        *rewritten += 1;
                        continue;
                    }
                }
                wrap_nullable_refs(value, nullable, rewritten);
            }
        }
        Value::Array(items) => {
            for item in items {
                wrap_nullable_refs(item, nullable, rewritten);
            }
        }
        _ => {}
    }
}

fn pascal_case(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(schemas: &Value) -> Value {
        json!({ "openapi": "3.1.0", "paths": {}, "components": { "schemas": schemas } })
    }

    #[test]
    fn nullable_refs_become_optional() {
        let mut doc = spec(&json!({
            "Filter": { "type": ["object", "null"], "properties": {} },
            "Campaign": {
                "type": "object",
                "properties": { "filter": { "$ref": "#/components/schemas/Filter", "description": "d" } },
            },
        }));
        normalize(&mut doc).unwrap();
        assert_eq!(
            doc.pointer("/components/schemas/Campaign/properties/filter")
                .unwrap(),
            &json!({
                "anyOf": [{ "$ref": "#/components/schemas/Filter" }, { "type": "null" }],
                "description": "d",
            })
        );
    }

    #[test]
    fn nullable_enums_are_hoisted_without_the_null_entry() {
        let mut doc = spec(&json!({
            "Pattern": {
                "type": "object",
                "properties": {
                    "platform": {
                        "type": ["string", "null"],
                        "description": "d",
                        "enum": ["clerk", "stripe", "null"],
                    },
                },
            },
            "Summary": {
                "type": "object",
                "properties": {
                    "platform": { "type": ["string", "null"], "enum": ["clerk", "stripe", null] },
                },
            },
        }));
        normalize(&mut doc).unwrap();
        assert_eq!(
            doc.pointer("/components/schemas/Platform").unwrap(),
            &json!({ "type": "string", "enum": ["clerk", "stripe"] })
        );
        let reference = json!([{ "$ref": "#/components/schemas/Platform" }, { "type": "null" }]);
        assert_eq!(
            doc.pointer("/components/schemas/Pattern/properties/platform/anyOf")
                .unwrap(),
            &reference
        );
        assert_eq!(
            doc.pointer("/components/schemas/Summary/properties/platform/anyOf")
                .unwrap(),
            &reference
        );
    }

    #[test]
    fn discriminators_without_mapping_get_one_from_member_enums() {
        let mut doc = spec(&json!({
            "Node": {
                "oneOf": [
                    { "$ref": "#/components/schemas/Exit" },
                    { "$ref": "#/components/schemas/Timer" },
                ],
                "discriminator": { "propertyName": "typeName" },
            },
            "Exit": { "properties": { "typeName": { "type": "string", "enum": ["ExitAction"] } } },
            "Timer": { "properties": { "typeName": { "type": "string", "enum": ["TimerAction"] } } },
        }));
        normalize(&mut doc).unwrap();
        assert_eq!(
            doc.pointer("/components/schemas/Node/discriminator/mapping")
                .unwrap(),
            &json!({
                "ExitAction": "#/components/schemas/Exit",
                "TimerAction": "#/components/schemas/Timer",
            })
        );
    }

    #[test]
    fn closed_objects_are_opened() {
        let mut doc = spec(&json!({
            "Node": {
                "type": "object",
                "additionalProperties": false,
                "properties": { "nested": { "type": "object", "additionalProperties": false } },
            },
            "Map": { "type": "object", "additionalProperties": { "type": "string" } },
        }));
        normalize(&mut doc).unwrap();
        let schemas = doc.pointer("/components/schemas").unwrap();
        assert!(schemas["Node"].get("additionalProperties").is_none());
        assert!(
            schemas["Node"]["properties"]["nested"]
                .get("additionalProperties")
                .is_none()
        );
        assert_eq!(
            schemas["Map"]["additionalProperties"],
            json!({ "type": "string" })
        );
    }

    #[test]
    fn discriminator_member_without_a_single_tag_fails() {
        let mut doc = spec(&json!({
            "Node": {
                "oneOf": [{ "$ref": "#/components/schemas/Loose" }],
                "discriminator": { "propertyName": "typeName" },
            },
            "Loose": { "properties": { "typeName": { "type": "string" } } },
        }));
        assert!(normalize(&mut doc).unwrap_err().contains("single string"));
    }

    #[test]
    fn conflicting_hoisted_enums_get_distinct_names() {
        let mut doc = spec(&json!({
            "A": { "properties": { "status": { "type": ["string", "null"], "enum": ["a"] } } },
            "B": { "properties": { "status": { "type": ["string", "null"], "enum": ["b"] } } },
        }));
        normalize(&mut doc).unwrap();
        assert_eq!(
            doc.pointer("/components/schemas/Status/enum").unwrap(),
            &json!(["a"])
        );
        assert_eq!(
            doc.pointer("/components/schemas/Status2/enum").unwrap(),
            &json!(["b"])
        );
    }

    #[test]
    fn hoisting_onto_an_existing_schema_name_fails() {
        let mut doc = spec(&json!({
            "Status": { "type": "string" },
            "A": { "properties": { "status": { "type": ["string", "null"], "enum": ["a"] } } },
        }));
        assert!(normalize(&mut doc).unwrap_err().contains("already exists"));
    }
}
