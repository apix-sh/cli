use oas3::spec::{ObjectSchema, SchemaType, SchemaTypeSet};

/// Returns the primary (non-null) type of a schema
pub fn primary_type(schema: &ObjectSchema) -> Option<SchemaType> {
    match &schema.schema_type {
        Some(SchemaTypeSet::Single(t)) => {
            if *t == SchemaType::Null {
                None
            } else {
                Some(*t)
            }
        }
        Some(SchemaTypeSet::Multiple(types)) => {
            types.iter().find(|t| **t != SchemaType::Null).cloned()
        }
        None => None,
    }
}

/// Returns true if the schema allows null
#[allow(dead_code)]
pub fn is_nullable(schema: &ObjectSchema) -> bool {
    match &schema.schema_type {
        Some(SchemaTypeSet::Multiple(types)) => types.contains(&SchemaType::Null),
        _ => false,
    }
}

/// Returns true if the schema is primarily a specific type
pub fn is_type(schema: &ObjectSchema, ty: SchemaType) -> bool {
    primary_type(schema) == Some(ty)
}

/// Returns true if the schema uses composition (allOf/anyOf/oneOf)
#[allow(dead_code)]
pub fn is_composition(schema: &ObjectSchema) -> bool {
    !schema.all_of.is_empty() || !schema.any_of.is_empty() || !schema.one_of.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primary_type() {
        let mut schema = ObjectSchema::default();

        // Single type
        schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::String));
        assert_eq!(primary_type(&schema), Some(SchemaType::String));

        // Multiple types including null
        schema.schema_type = Some(SchemaTypeSet::Multiple(vec![
            SchemaType::String,
            SchemaType::Null,
        ]));
        assert_eq!(primary_type(&schema), Some(SchemaType::String));
        assert!(is_nullable(&schema));

        // Just null
        schema.schema_type = Some(SchemaTypeSet::Single(SchemaType::Null));
        assert_eq!(primary_type(&schema), None);

        // None
        schema.schema_type = None;
        assert_eq!(primary_type(&schema), None);
    }
}
