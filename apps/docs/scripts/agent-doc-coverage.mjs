import ts from "typescript";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const sourceRoot = fileURLToPath(
  new URL("../../../sdks/js/agent-sdk/src/", import.meta.url),
);
const options = {
  noEmit: true,
  skipLibCheck: true,
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler,
};

function hasDoc(node) {
  return ts.getJSDocCommentsAndTags(node).some((doc) => {
    if (!ts.isJSDoc(doc)) return false;
    const comment =
      typeof doc.comment === "string"
        ? doc.comment
        : doc.comment?.map((part) => part.text ?? "").join("");
    return Boolean(comment?.trim());
  });
}

function isPrivate(node) {
  return (
    (node.name && ts.isPrivateIdentifier(node.name)) ||
    node.modifiers?.some(
      (modifier) =>
        modifier.kind === ts.SyntaxKind.PrivateKeyword ||
        modifier.kind === ts.SyntaxKind.ProtectedKeyword,
    )
  );
}

export function checkProgram(program, rootPath, ownerRoot) {
  const checker = program.getTypeChecker();
  const root = program.getSourceFile(rootPath);
  if (!root) throw new Error(`API entry point is missing: ${rootPath}`);
  const module = checker.getSymbolAtLocation(root);
  if (!module) throw new Error(`API entry point is not a module: ${rootPath}`);
  const missing = [];
  const seen = new Set();
  const seenTypes = new Set();
  const owned = (node) =>
    node.getSourceFile().fileName.startsWith(`${resolve(ownerRoot)}/`);

  function requireDoc(node) {
    if (hasDoc(node)) return;
    const source = node.getSourceFile();
    const line = source.getLineAndCharacterOfPosition(node.getStart()).line + 1;
    missing.push(
      `${source.fileName}:${line}: missing TSDoc for ${node.name?.getText() ?? ts.SyntaxKind[node.kind]}`,
    );
  }

  function followSymbol(symbol) {
    if (!symbol) return;
    if (symbol.flags & ts.SymbolFlags.Alias)
      symbol = checker.getAliasedSymbol(symbol);
    for (const declaration of symbol.declarations ?? []) {
      if (owned(declaration)) visitDeclaration(declaration);
    }
  }

  // Inferred return values and exported function variables are also public API.
  function visitResolvedType(type) {
    if (!type || seenTypes.has(type)) return;
    seenTypes.add(type);
    followSymbol(type.aliasSymbol);
    if (
      type.symbol?.declarations?.some(
        (node) =>
          owned(node) &&
          (ts.isClassDeclaration(node) ||
            ts.isInterfaceDeclaration(node) ||
            ts.isEnumDeclaration(node)),
      )
    )
      followSymbol(type.symbol);
    for (const argument of type.aliasTypeArguments ?? [])
      visitResolvedType(argument);
    if (
      type.flags & ts.TypeFlags.Object &&
      type.objectFlags & ts.ObjectFlags.Reference
    ) {
      for (const argument of checker.getTypeArguments(type))
        visitResolvedType(argument);
    }
    if (type.isUnionOrIntersection()) {
      for (const part of type.types) visitResolvedType(part);
    }
    for (const kind of [ts.SignatureKind.Call, ts.SignatureKind.Construct]) {
      for (const signature of checker.getSignaturesOfType(type, kind)) {
        const declaration = signature.getDeclaration();
        if (!declaration || !owned(declaration)) continue;
        visitSignatureTypes(declaration);
        visitResolvedType(checker.getTypePredicateOfSignature(signature)?.type);
        for (const parameter of signature.getParameters()) {
          const node = parameter.valueDeclaration ?? declaration;
          visitResolvedType(checker.getTypeOfSymbolAtLocation(parameter, node));
        }
        visitResolvedType(checker.getReturnTypeOfSignature(signature));
      }
    }
    for (const property of checker.getPropertiesOfType(type)) {
      for (const declaration of property.declarations ?? []) {
        if (owned(declaration) && !isPrivate(declaration))
          visitDeclaration(declaration);
      }
    }
  }

  function visitSignatureTypes(node) {
    visitType(node.type);
    for (const parameter of node.parameters ?? []) visitType(parameter.type);
    for (const parameter of node.typeParameters ?? []) {
      visitType(parameter.constraint);
      visitType(parameter.default);
    }
  }

  // Follow public type syntax, not implementation bodies or private state.
  function visitType(node) {
    if (!node) return;
    if (ts.isTypeLiteralNode(node)) {
      for (const member of node.members) visitDeclaration(member);
      return;
    }
    if (ts.isIdentifier(node)) {
      const symbol = checker.getSymbolAtLocation(node);
      const target =
        symbol?.flags & ts.SymbolFlags.Alias
          ? checker.getAliasedSymbol(symbol)
          : symbol;
      if (
        target?.declarations?.some(
          (declaration) =>
            ts.isTypeAliasDeclaration(declaration) ||
            ts.isInterfaceDeclaration(declaration) ||
            ts.isClassDeclaration(declaration) ||
            ts.isEnumDeclaration(declaration),
        )
      )
        followSymbol(symbol);
    }
    ts.forEachChild(node, visitType);
  }

  function visitDeclaration(node) {
    if (seen.has(node) || !owned(node) || isPrivate(node)) return;
    seen.add(node);
    requireDoc(node);
    visitResolvedType(checker.getTypeAtLocation(node));
    if (ts.isClassDeclaration(node) || ts.isInterfaceDeclaration(node)) {
      for (const member of node.members) {
        if (!ts.isClassStaticBlockDeclaration(member)) visitDeclaration(member);
      }
      for (const clause of node.heritageClauses ?? []) visitType(clause);
    }
    if (ts.isEnumDeclaration(node)) {
      for (const member of node.members) visitDeclaration(member);
    }
    visitSignatureTypes(node);
    for (const parameter of node.parameters ?? []) {
      if (
        ts.isParameterPropertyDeclaration(parameter, node) &&
        !isPrivate(parameter)
      )
        requireDoc(parameter);
    }
    if (
      ts.isVariableDeclaration(node) &&
      node.initializer &&
      ts.isObjectLiteralExpression(node.initializer)
    ) {
      for (const member of node.initializer.properties)
        visitDeclaration(member);
    }
    if (ts.isShorthandPropertyAssignment(node)) {
      const value =
        checker.getShorthandAssignmentValueSymbol(node)?.valueDeclaration;
      if (value?.initializer && ts.isArrowFunction(value.initializer)) {
        visitType(value.initializer.type);
        for (const parameter of value.initializer.parameters)
          visitType(parameter.type);
      }
    }
  }

  const exports = checker.getExportsOfModule(module);
  if (!exports.length)
    throw new Error(`API entry point has no resolved exports: ${rootPath}`);
  for (const symbol of exports) followSymbol(symbol);
  return [...new Set(missing)];
}

export function checkSources(sources) {
  const input = [...sources.keys()];
  const defaults = ts.createCompilerHost(options);
  const host = {
    ...defaults,
    directoryExists: (path) =>
      input.some((file) => file.startsWith(`${path}/`)) ||
      defaults.directoryExists(path),
    fileExists: (file) => sources.has(file) || defaults.fileExists(file),
    readFile: (file) => sources.get(file) ?? defaults.readFile(file),
    getSourceFile: (file, version) =>
      sources.has(file)
        ? ts.createSourceFile(file, sources.get(file), version, true)
        : defaults.getSourceFile(file, version),
  };
  return checkProgram(
    ts.createProgram(input, options, host),
    "/fixture/index.ts",
    "/fixture",
  );
}

export async function findUndocumented() {
  const configPath = join(sourceRoot, "../tsconfig.json");
  const config = ts.readConfigFile(configPath, ts.sys.readFile);
  if (config.error)
    throw new Error(
      ts.flattenDiagnosticMessageText(config.error.messageText, "\n"),
    );
  const parsed = ts.parseJsonConfigFileContent(
    config.config,
    ts.sys,
    dirname(configPath),
  );
  if (parsed.errors.length)
    throw new Error(
      parsed.errors
        .map((error) =>
          ts.flattenDiagnosticMessageText(error.messageText, "\n"),
        )
        .join("\n"),
    );
  return checkProgram(
    ts.createProgram(parsed.fileNames, { ...parsed.options, ...options }),
    join(sourceRoot, "index.ts"),
    sourceRoot,
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const missing = await findUndocumented();
  if (missing.length) {
    console.error(missing.join("\n"));
    process.exitCode = 1;
  } else console.log("Agent SDK public declarations have TSDoc.");
}
