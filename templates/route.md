---
method: {{ method }}
url: {{ url }}
auth: {{ auth }}
content_type: {{ content_type }}
---

# {{ summary }}

{{ description }}

## Path Parameters
{% if path_params.is_empty() %}
*(None)*
{% else %}
| :--- | :---: | :--- | :--- |
| Name | Required | Type | Description |
| :--- | :---: | :--- | :--- |
{% for p in path_params %}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor %}
| :--- | :---: | :--- | :--- |
{% endif %}

## Query Parameters
{% if query_params.is_empty() %}
*(None)*
{% else %}
| :--- | :---: | :--- | :--- |
| Name | Required | Type | Description |
| :--- | :---: | :--- | :--- |
{% for p in query_params %}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor %}
| :--- | :---: | :--- | :--- |
{% endif %}

## Request Body
{% if request_body.is_empty() %}
*(None)*
{% else %}
{{ request_body }}
{% endif %}

## Responses
{% for r in responses %}
* **{{ r.status }}**: {{ r.description }}
{% endfor %}
