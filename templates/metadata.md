---
base_url: "{{ base_url }}"
auth: "{{ auth }}"
tags: [{{ tags|join(", ") }}]
---

# {{ title }}

{% if !description.is_empty() %}
{{ description }}

{% endif -%}
**Version:** {{ version }}
**Base URL:** `{{ base_url }}`
