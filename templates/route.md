---
method: "{{ method }}"
url: "{{ url }}"
{%- match auth %}
{%- when Some with (auth_val) %}
auth: "{{ auth_val }}"
{%- when None %}
{%- endmatch %}
content_type: "{{ content_type }}"
---

# {{ summary }}

{% if !description.is_empty() -%}
{{ description }}

{% endif -%}

## Path Parameters

{% if path_params.is_empty() -%}
_(None)_
{% else -%}
| Name | Required | Type | Description |
| :--- | :------: | :--- | :---------- |
{% for p in path_params -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor %}
{%- endif %}

## Query Parameters

{% if query_params.is_empty() -%}
_(None)_
{% else -%}
| Name | Required | Type | Description |
| :--- | :------: | :--- | :---------- |
{% for p in query_params -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor %}
{%- endif %}
{% if !header_params.is_empty() -%}

## Header Parameters

| Name | Required | Type | Description |
| :--- | :------: | :--- | :---------- |
{% for p in header_params -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor %}
{%- endif -%}
{% if !cookie_params.is_empty() -%}

## Cookie Parameters

| Name | Required | Type | Description |
| :--- | :------: | :--- | :---------- |
{% for p in cookie_params -%}
| `{{ p.name }}` | {{ p.required }} | {{ p.param_type }} | {{ p.description }} |
{% endfor %}
{%- endif %}

## Request Body

{% if request_body.is_empty() -%}
_(None)_
{% else -%}
{{ request_body }}
{% endif %}

## Responses

{% for r in responses -%}

### {{ r.status }}

{% if !r.description.is_empty() -%}
{{ r.description }}

{% endif -%}
{% if !r.headers.is_empty() -%}

#### Headers

| Name | Required | Type | Description |
| :--- | :------: | :--- | :---------- |
{% for h in r.headers -%}
| `{{ h.name }}` | {{ h.required }} | {{ h.param_type }} | {{ h.description }} |
{% endfor %}

{% endif -%}
{% if !r.content.is_empty() -%}
{{ r.content }}

{% endif -%}
{% endfor -%}
