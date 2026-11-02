import { readFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import ts from "typescript";

const allowed: Record<string, string[]> = {
  protocol: [],
  runtime: ["protocol"],
  platform: ["protocol", "runtime"],
  providers: ["protocol", "runtime", "platform"],
  host: ["protocol", "runtime", "platform", "providers"],
  "linux-host": ["protocol", "runtime"],
};
for (const [name, dependencies] of Object.entries(allowed)) {
  const root = resolve("packages", name),
    manifest: unknown = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  if (
    !manifest ||
    typeof manifest !== "object" ||
    !("dependencies" in manifest) ||
    !manifest.dependencies ||
    typeof manifest.dependencies !== "object"
  )
    throw new Error(`invalid ${name} manifest`);
  const declared = Object.keys(manifest.dependencies).filter((key) => key.startsWith("@kite/"));
  if (declared.some((key) => !dependencies.includes(key.slice(6))))
    throw new Error(`invalid ${name} package dependency`);
  for (const path of new Bun.Glob("{src,test}/**/*.ts").scanSync({ cwd: root, absolute: true })) {
    const production = relative(root, path).startsWith("src/");
    if (production && (path.endsWith(".test.ts") || /\/fixtures?\//.test(path)))
      throw new Error(`${path} must be under its package test directory`);
    const source = readFileSync(path, "utf8"),
      file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
    const checkImport = (specifier: string): void => {
      if (specifier.startsWith("@kite/")) {
        const dependency = specifier.slice(6).split("/")[0] || "";
        if (!dependencies.includes(dependency) || !declared.includes(`@kite/${dependency}`))
          throw new Error(`${path} imports undeclared ${specifier}`);
      }
      if (specifier.startsWith(".")) {
        const target = relative(root, resolve(dirname(path), specifier));
        if (target === ".." || target.startsWith("../") || isAbsolute(target))
          throw new Error(`${path} crosses its package using ${specifier}`);
        if (production && (target.startsWith("test/") || target.endsWith(".test.ts")))
          throw new Error(`${path} imports test code using ${specifier}`);
      }
      if (production && specifier === "bun:test")
        throw new Error(`${path} imports the test runtime`);
    };
    const visit = (node: ts.Node): void => {
      if (
        (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
        node.moduleSpecifier &&
        ts.isStringLiteral(node.moduleSpecifier)
      )
        checkImport(node.moduleSpecifier.text);
      if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
        const first = node.arguments[0];
        if (first && ts.isStringLiteral(first)) checkImport(first.text);
      }
      ts.forEachChild(node, visit);
    };
    visit(file);
    if (/(?:\.py\b|python3\b)/.test(source) && production)
      throw new Error(`${path} contains a Python runtime dependency`);
  }
}
for (const path of new Bun.Glob("**/*.py").scanSync({
  cwd: resolve("desktop/linux"),
  absolute: true,
}))
  throw new Error(`${path} belongs in the Bun Linux host package`);
console.log("Checked server package dependency boundaries and native-client separation");
