import { readFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import ts from "typescript";

const allowed: Record<string, string[]> = {
  protocol: [],
  runtime: ["protocol"],
  platform: ["protocol", "runtime"],
  providers: ["protocol", "runtime", "platform"],
  host: ["protocol", "runtime", "platform", "providers"],
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
  for (const path of new Bun.Glob("src/**/*.ts").scanSync({ cwd: root, absolute: true })) {
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
      }
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
    if (/(?:\.py\b|python3\b)/.test(source) && !path.endsWith(".test.ts"))
      throw new Error(`${path} contains a Python runtime dependency`);
  }
}
console.log("Checked server package dependency boundaries and native-client separation");
