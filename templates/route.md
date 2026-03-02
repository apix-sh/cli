---
method: "{{ method }}"
url: "{{ url }}"
auth: "{{ auth }}"
content_type: "{{ content_type }}"
---

# {{ summary }}

{{ description }}

## Path Parameters

{% if path_params.is_empty() %}
_(None)_
{% else %}
| Name | Required | Type | Description |
| :--- | :---: | :--- | :--- |
{% for p in path_params -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor -%}
{% endif %}

## Query Parameters

{% if query_params.is_empty() %}
_(None)_
{% else %}
| Name | Required | Type | Description |
| :--- | :---: | :--- | :--- |
{% for p in query_params -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor -%}
{% endif %}

{% if !header_params.is_empty() -%}

## Header Parameters

| Name | Required | Type | Description |
| :--- | :------: | :--- | :---------- |

{% for p in header_params -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor -%}
{% endif %}

{% if !cookie_params.is_empty() -%}

## Cookie Parameters

| Name | Required | Type | Description |
| :--- | :------: | :--- | :---------- |

{% for p in cookie_params -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor -%}
{% endif %}

## Request Body

{% if request_body.is_empty() %}
_(None)_
{% else %}
{{ request_body }}
{% endif %}

## Responses

{% for r in responses %}

- **{{ r.status }}**: {{ r.description }}
  {% endfor %}
