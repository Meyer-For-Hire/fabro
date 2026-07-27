import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

function fail(message) {
  throw new Error(message);
}

function globRegex(glob) {
  let source = "^";
  for (let index = 0; index < glob.length; index += 1) {
    const character = glob[index];
    if (character === "*") {
      if (glob[index + 1] === "*") {
        source += ".*";
        index += 1;
      } else {
        source += "[^/]*";
      }
    } else if (character === "?") {
      source += "[^/]";
    } else {
      source += character.replace(/[\\^$.*+?()[\]{}|]/g, "\\$&");
    }
  }
  return new RegExp(`${source}$`);
}

function matchesAny(path, globs) {
  return globs.some((glob) => globRegex(glob).test(path));
}

function requireKeys(value, expected, label) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    fail(
      `${label} keys differ\nexpected ${JSON.stringify(wanted)}\nfound ${JSON.stringify(actual)}`,
    );
  }
}

function referencedPath(reference) {
  return reference.split(" — ", 1)[0].split(":", 1)[0];
}

function validate(map) {
  requireKeys(
    map,
    [
      "schema_version",
      "cartography_version",
      "created_at",
      "repository",
      "instructions",
      "overview",
      "global_exclusions",
      "components",
      "unmapped_files",
      "coverage",
      "open_questions",
    ],
    "map",
  );
  requireKeys(
    map.repository,
    ["name", "root", "revision", "short_revision"],
    "repository",
  );
  requireKeys(
    map.coverage,
    [
      "relevant_file_count",
      "assigned_file_count",
      "excluded_file_count",
      "unmapped_file_count",
    ],
    "coverage",
  );
  if (map.schema_version !== 1) fail("schema_version must be 1");
  if (map.cartography_version !== 1) fail("cartography_version must be 1");

  const files = execFileSync(
    "git",
    ["ls-tree", "-r", "--name-only", map.repository.revision],
    { encoding: "utf8" },
  )
    .trim()
    .split("\n")
    .filter(Boolean);
  const fileSet = new Set(files);
  const ids = map.components.map(({ id }) => id);
  if (new Set(ids).size !== ids.length) fail("component IDs are not unique");
  for (const id of ids) {
    if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(id)) {
      fail(`invalid component ID: ${id}`);
    }
  }

  const allGlobs = [];
  for (const exclusion of map.global_exclusions) {
    requireKeys(exclusion, ["globs", "reason"], "global exclusion");
    allGlobs.push(...exclusion.globs);
  }
  for (const component of map.components) {
    requireKeys(
      component,
      [
        "id",
        "name",
        "purpose",
        "globs",
        "exclude_globs",
        "entry_points",
        "owns",
        "depends_on",
        "evidence",
      ],
      `component ${component.id}`,
    );
    allGlobs.push(...component.globs, ...component.exclude_globs);
    for (const dependency of component.depends_on) {
      if (!ids.includes(dependency)) {
        fail(`${component.id} depends on missing component ${dependency}`);
      }
      if (dependency === component.id) {
        fail(`${component.id} depends on itself`);
      }
    }
    for (const reference of [...component.entry_points, ...component.evidence]) {
      const path = referencedPath(reference);
      if (!fileSet.has(path)) {
        fail(`${component.id} references missing path ${path}`);
      }
    }
  }
  for (const instruction of map.instructions) {
    if (!fileSet.has(instruction)) fail(`missing instruction ${instruction}`);
  }
  for (const glob of allGlobs) {
    if (!files.some((path) => globRegex(glob).test(path))) {
      fail(`glob resolves to no tracked files: ${glob}`);
    }
  }

  const excluded = new Set(
    files.filter((path) =>
      map.global_exclusions.some(({ globs }) => matchesAny(path, globs)),
    ),
  );
  const claims = new Map();
  for (const component of map.components) {
    for (const path of files) {
      if (
        matchesAny(path, component.globs) &&
        !matchesAny(path, component.exclude_globs)
      ) {
        if (excluded.has(path)) {
          fail(`${path} is both globally excluded and claimed by ${component.id}`);
        }
        const previous = claims.get(path);
        if (previous) {
          fail(`${path} is claimed by both ${previous} and ${component.id}`);
        }
        claims.set(path, component.id);
      }
    }
  }

  for (const path of map.unmapped_files) {
    if (!fileSet.has(path)) fail(`unmapped file does not exist: ${path}`);
    if (excluded.has(path) || claims.has(path)) {
      fail(`unmapped file also has another disposition: ${path}`);
    }
  }
  const unmapped = new Set(map.unmapped_files);
  const missing = files.filter(
    (path) => !claims.has(path) && !excluded.has(path) && !unmapped.has(path),
  );
  if (missing.length > 0) {
    fail(`files lack a disposition:\n${missing.join("\n")}`);
  }

  const computed = {
    relevant_file_count: files.length,
    assigned_file_count: claims.size,
    excluded_file_count: excluded.size,
    unmapped_file_count: unmapped.size,
  };
  if (JSON.stringify(computed) !== JSON.stringify(map.coverage)) {
    fail(
      `coverage mismatch\nexpected ${JSON.stringify(computed)}\nfound ${JSON.stringify(map.coverage)}`,
    );
  }
  if (
    computed.assigned_file_count +
      computed.excluded_file_count +
      computed.unmapped_file_count !==
    computed.relevant_file_count
  ) {
    fail("coverage counts do not add up");
  }
  return computed;
}

function inline(values) {
  return values.map((value) => `\`${value}\``).join(", ");
}

function render(map) {
  const lines = [
    "# Chisel Codebase Map",
    "",
    `Cartography v${map.cartography_version} · revision \`${map.repository.revision}\` · ${map.created_at}`,
    `Assigned ${map.coverage.assigned_file_count} files · excluded ${map.coverage.excluded_file_count} · unmapped ${map.coverage.unmapped_file_count} · instructions: ${map.instructions.join(", ")}`,
    "",
    map.overview,
    "",
    "## Components",
  ];

  for (const component of map.components) {
    lines.push(
      "",
      `### \`${component.id}\` — ${component.name}`,
      "",
      `- **Purpose:** ${component.purpose}`,
      `- **Paths:** ${inline(component.globs)}`,
    );
    if (component.exclude_globs.length > 0) {
      lines.push(`- **Excludes:** ${inline(component.exclude_globs)}`);
    }
    if (component.entry_points.length > 0) {
      lines.push(`- **Entry points:** ${inline(component.entry_points)}`);
    }
    if (component.owns.length > 0) {
      lines.push(`- **Owns:** ${component.owns.join("; ")}`);
    }
    if (component.depends_on.length > 0) {
      lines.push(`- **Depends on:** ${inline(component.depends_on)}`);
    }
    if (component.evidence.length > 0) {
      lines.push(`- **Evidence:** ${component.evidence.join("; ")}`);
    }
  }

  if (map.global_exclusions.length > 0 || map.unmapped_files.length > 0) {
    lines.push("", "## Exclusions and Unmapped Code", "");
    for (const exclusion of map.global_exclusions) {
      lines.push(`- ${inline(exclusion.globs)} — ${exclusion.reason}`);
    }
    for (const path of map.unmapped_files) {
      lines.push(`- \`${path}\` — unmapped`);
    }
  }

  if (map.open_questions.length > 0) {
    lines.push("", "## Open Questions", "");
    for (const question of map.open_questions) {
      lines.push(`- ${question}`);
    }
  }
  lines.push("");
  return lines.join("\n");
}

const [inputPath, outputPath] = process.argv.slice(2);
if (!inputPath) fail("usage: validate-render.mjs <map.json> [map.md]");
const map = JSON.parse(readFileSync(inputPath, "utf8"));
const coverage = validate(map);
if (outputPath) writeFileSync(outputPath, render(map));
process.stdout.write(`${JSON.stringify(coverage)}\n`);
