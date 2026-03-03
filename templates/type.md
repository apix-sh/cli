---
type: {{ schema_type }}
---

# {{ name }}
{% if !description.is_empty() %}
{{ description }}
{% endif %}
## Properties

| Property | Required | Type | Description |
| :--- | :---: | :--- | :--- |
{% for p in properties -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.prop_type }} | {{ p.description }} |
{% endfor -%}