import type { McpElicitationField, McpElicitationOption, McpElicitationSchema } from "$lib/types";

/**
 * A field flattened into the one shape the card draws. Upstream models the
 * schema as nested untagged unions — a single select is `enum` + `enumNames`
 * on some servers and `oneOf` on others, a multi select is `type: "array"`
 * with the same two shapes buried under `items` — so the card would otherwise
 * have to re-derive the control from four places at render time.
 */
export interface FormField {
  name: string;
  control: "text" | "number" | "boolean" | "select" | "multiselect";
  label: string;
  description?: string;
  required: boolean;
  options: McpElicitationOption[];
  /** Only ever set for `text`, to pick the right input type. */
  format?: string;
  default?: string | number | boolean | string[];
}

function options(source: McpElicitationField | NonNullable<McpElicitationField["items"]>): McpElicitationOption[] {
  const titled = source.oneOf ?? ("anyOf" in source ? source.anyOf : undefined);
  if (titled?.length) return titled;
  const values = source.enum ?? [];
  const names = "enumNames" in source ? source.enumNames : undefined;
  return values.map((value, index) => ({ const: value, title: names?.[index] ?? value }));
}

function control(field: McpElicitationField): FormField["control"] {
  if (field.type === "array") return "multiselect";
  if (field.type === "boolean") return "boolean";
  if (options(field).length) return "select";
  if (field.type === "number" || field.type === "integer") return "number";
  return "text";
}

export function formFields(schema: McpElicitationSchema | null | undefined): FormField[] {
  const required = new Set(schema?.required ?? []);
  return Object.entries(schema?.properties ?? {}).map(([name, field]) => {
    const kind = control(field);
    return {
      name,
      control: kind,
      label: field.title ?? name,
      description: field.description,
      required: required.has(name),
      options: kind === "multiselect" ? options(field.items ?? {}) : options(field),
      format: field.format,
      default: field.default,
    };
  });
}

/** Whether every required field has been given a value. */
export function isComplete(fields: FormField[], values: Record<string, unknown>): boolean {
  return fields.every((field) => {
    if (!field.required) return true;
    const value = values[field.name];
    if (field.control === "boolean") return typeof value === "boolean";
    if (field.control === "multiselect") return Array.isArray(value) && value.length > 0;
    return value != null && String(value).trim().length > 0;
  });
}

/**
 * The `content` object sent back on accept. Numbers go back as numbers and
 * untouched optional fields are omitted entirely, since the MCP server
 * validates this against the schema it sent.
 */
export function buildContent(fields: FormField[], values: Record<string, unknown>): Record<string, unknown> {
  const content: Record<string, unknown> = {};
  for (const field of fields) {
    const value = values[field.name];
    if (value == null || value === "") continue;
    if (field.control === "number") {
      const parsed = Number(value);
      if (!Number.isNaN(parsed)) content[field.name] = parsed;
      continue;
    }
    if (field.control === "multiselect") {
      if (Array.isArray(value) && value.length) content[field.name] = value;
      continue;
    }
    content[field.name] = value;
  }
  return content;
}
