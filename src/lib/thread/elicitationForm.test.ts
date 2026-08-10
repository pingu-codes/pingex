import { describe, expect, it } from "vitest";
import { buildContent, formFields, isComplete } from "$lib/thread/elicitationForm";
import type { McpElicitationSchema } from "$lib/types";

const schema = (properties: McpElicitationSchema["properties"], required?: string[]) =>
  ({ type: "object", properties, ...(required ? { required } : {}) }) satisfies McpElicitationSchema;

describe("formFields", () => {
  it("picks a control per field type", () => {
    const fields = formFields(
      schema({
        name: { type: "string" },
        age: { type: "integer" },
        subscribed: { type: "boolean" },
      }),
    );

    expect(fields.map((field) => field.control)).toEqual(["text", "number", "boolean"]);
  });

  // A single select reaches the app two ways: bare `enum` values with optional
  // display names alongside, or `oneOf` with the title already attached.
  it("normalises enum + enumNames into titled options", () => {
    const [field] = formFields(schema({ tier: { type: "string", enum: ["free", "pro"], enumNames: ["Free", "Pro"] } }));

    expect(field.control).toBe("select");
    expect(field.options).toEqual([
      { const: "free", title: "Free" },
      { const: "pro", title: "Pro" },
    ]);
  });

  it("normalises oneOf into the same options", () => {
    const [field] = formFields(schema({ tier: { type: "string", oneOf: [{ const: "pro", title: "Pro" }] } }));

    expect(field.options).toEqual([{ const: "pro", title: "Pro" }]);
  });

  it("falls back to the raw value when no display name is given", () => {
    const [field] = formFields(schema({ tier: { type: "string", enum: ["free"] } }));

    expect(field.options).toEqual([{ const: "free", title: "free" }]);
  });

  // Multi-selects hide their choices a level down, under `items`.
  it("reads a multiselect's options out of items", () => {
    const [field] = formFields(schema({ tags: { type: "array", items: { type: "string", enum: ["a", "b"] } } }));

    expect(field.control).toBe("multiselect");
    expect(field.options.map((option) => option.const)).toEqual(["a", "b"]);
  });

  it("reads a titled multiselect's options out of items.anyOf", () => {
    const [field] = formFields(schema({ tags: { type: "array", items: { anyOf: [{ const: "a", title: "Ay" }] } } }));

    expect(field.options).toEqual([{ const: "a", title: "Ay" }]);
  });

  it("marks required fields and falls back to the key as a label", () => {
    const fields = formFields(schema({ name: { type: "string" }, note: { type: "string" } }, ["name"]));

    expect(fields.map((field) => [field.label, field.required])).toEqual([
      ["name", true],
      ["note", false],
    ]);
  });

  it("returns nothing for a missing schema", () => {
    expect(formFields(undefined)).toEqual([]);
  });
});

describe("isComplete", () => {
  const fields = formFields(
    schema(
      {
        name: { type: "string" },
        subscribed: { type: "boolean" },
        tags: { type: "array", items: { type: "string", enum: ["a"] } },
        note: { type: "string" },
      },
      ["name", "subscribed", "tags"],
    ),
  );

  it("requires a value for every required field", () => {
    expect(isComplete(fields, {})).toBe(false);
    expect(isComplete(fields, { name: "x", subscribed: false, tags: ["a"] })).toBe(true);
  });

  // `false` is a real answer to a required checkbox; blank text and an empty
  // list are not.
  it("treats false as answered but blank and empty as not", () => {
    expect(isComplete(fields, { name: "  ", subscribed: false, tags: ["a"] })).toBe(false);
    expect(isComplete(fields, { name: "x", subscribed: false, tags: [] })).toBe(false);
  });

  it("ignores optional fields", () => {
    expect(isComplete(fields, { name: "x", subscribed: true, tags: ["a"], note: "" })).toBe(true);
  });
});

describe("buildContent", () => {
  const fields = formFields(
    schema({
      name: { type: "string" },
      age: { type: "integer" },
      subscribed: { type: "boolean" },
      tags: { type: "array", items: { type: "string", enum: ["a"] } },
    }),
  );

  it("sends numbers as numbers and drops untouched fields", () => {
    expect(buildContent(fields, { name: "Ada", age: "41", subscribed: false })).toEqual({
      name: "Ada",
      age: 41,
      subscribed: false,
    });
  });

  it("omits empty strings and empty selections", () => {
    expect(buildContent(fields, { name: "", tags: [] })).toEqual({});
  });

  it("drops a number that will not parse rather than sending a string", () => {
    expect(buildContent(fields, { age: "not a number" })).toEqual({});
  });
});
