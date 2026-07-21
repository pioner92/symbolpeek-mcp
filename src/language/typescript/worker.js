const fs = require("node:fs");
const path = require("node:path");

const request = JSON.parse(fs.readFileSync(0, "utf8"));
const ts = loadTypeScript();

// Prefer the project's own TypeScript so parsing and resolution match the
// version the project actually compiles with (newer syntax, version-specific
// module resolution). Fall back to the bundled runtime when the project has
// none. Node walks up node_modules from `base`, so the file's directory is a
// sufficient starting point.
function loadTypeScript() {
  const base =
    request.workspace_root ||
    (request.path ? path.dirname(path.resolve(request.path)) : process.cwd());
  try {
    return require(require.resolve("typescript", { paths: [base] }));
  } catch {
    return require("typescript");
  }
}
const isWorkspaceOperation = request.operation === "search_symbols";
const workspaceRoot = path.resolve(
  request.workspace_root || (isWorkspaceOperation ? request.path : path.dirname(request.path)),
);
const currentFileName = isWorkspaceOperation
  ? path.join(workspaceRoot, "__symbolpeek_workspace__.ts")
  : path.resolve(request.path);
const scriptKind = {
  ts: ts.ScriptKind.TS,
  tsx: ts.ScriptKind.TSX,
  js: ts.ScriptKind.JS,
  jsx: ts.ScriptKind.JSX,
}[request.extension];

if (!isWorkspaceOperation && scriptKind === undefined) {
  process.stderr.write(`unsupported extension: ${request.extension}`);
  process.exit(2);
}

const sourceFile = isWorkspaceOperation
  ? undefined
  : ts.createSourceFile(
    currentFileName,
    request.source,
    ts.ScriptTarget.Latest,
    true,
    scriptKind,
  );

if (sourceFile && sourceFile.parseDiagnostics.length > 0 && request.operation !== "get_diagnostics") {
  const message = sourceFile.parseDiagnostics
    .map((diagnostic) => ts.flattenDiagnosticMessageText(diagnostic.messageText, " "))
    .join("; ");
  process.stderr.write(message);
  process.exit(3);
}

const definitions = [];
const definitionByName = new Map();
// Re-exports (`export ... from '...'`) have no local binding, so they live
// outside `definitions`/`definitionByName` to keep name resolution clean. They
// are merged into the emitted symbol list so barrel files aren't reported empty.
const reexports = [];

function byteOffset(utf16Offset) {
  return Buffer.byteLength(request.source.slice(0, utf16Offset), "utf8");
}

function span(node) {
  return { start: byteOffset(node.getStart(sourceFile)), end: byteOffset(node.getEnd()) };
}

function identifierSpan(node) {
  if (!node) return { start: 0, end: 0 };
  return span(node);
}

function nameOf(node) {
  if (!node) return undefined;
  if (ts.isIdentifier(node) || ts.isPrivateIdentifier(node)) return node.text;
  if (ts.isStringLiteral(node) || ts.isNumericLiteral(node)) return node.text;
  return undefined;
}

function declarationNames(node) {
  if (ts.isIdentifier(node)) return [{ name: node.text, node }];
  if (ts.isObjectBindingPattern(node) || ts.isArrayBindingPattern(node)) {
    return node.elements.flatMap((element) => {
      if (ts.isOmittedExpression(element)) return [];
      return declarationNames(element.name);
    });
  }
  return [];
}

function containsJsx(node) {
  let found = false;
  function walk(current) {
    if (ts.isJsxElement(current) || ts.isJsxSelfClosingElement(current) || ts.isJsxFragment(current)) {
      found = true;
      return;
    }
    ts.forEachChild(current, walk);
  }
  walk(node);
  return found;
}

function classifyFunction(name, isArrow, node) {
  const firstCharacter = name.charCodeAt(0);
  const hookCharacter = name.charCodeAt(3);
  const isUppercase = (character) => character >= 65 && character <= 90;
  if (name.slice(0, 3) === "use" && name.length > 3 && isUppercase(hookCharacter)) return "hook";
  if (isUppercase(firstCharacter) || containsJsx(node)) return "react_component";
  return isArrow ? "arrow_function" : "function";
}

function isReactWrapper(node) {
  if (!ts.isCallExpression(node)) return false;
  const expression = node.expression;
  const name = ts.isIdentifier(expression)
    ? expression.text
    : (ts.isPropertyAccessExpression(expression) ? expression.name.text : undefined);
  return name === "memo" || name === "forwardRef" || name === "lazy";
}

function variableKind(name, initializer, isConst) {
  if (initializer && (ts.isArrowFunction(initializer) || ts.isFunctionExpression(initializer))) {
    return { kind: classifyFunction(name, ts.isArrowFunction(initializer), initializer), category: "function" };
  }
  if (initializer && (containsJsx(initializer) || (name.length > 0 && name.charCodeAt(0) >= 65 && name.charCodeAt(0) <= 90 && isReactWrapper(initializer)))) {
    return { kind: "react_component", category: "function" };
  }
  return { kind: isConst ? "constant" : "variable", category: isConst ? "constant" : "other" };
}

function addDefinition(name, node, nameNode, kind, category, scope, topLevel) {
  if (!name) return;
  const qualifiedName = [...scope, name].join(".");
  const nodeSpan = span(node);
  const nameSpan = identifierSpan(nameNode || node.name);
  const existing = definitionByName.get(qualifiedName);
  if (existing) {
    existing.start = Math.min(existing.start, nodeSpan.start);
    existing.end = Math.max(existing.end, nodeSpan.end);
    if (ts.isFunctionDeclaration(node) && node.body) existing.node = node;
    return;
  }
  const definition = {
    name: qualifiedName,
    kind,
    category,
    start: nodeSpan.start,
    end: nodeSpan.end,
    name_start: nameSpan.start,
    name_end: nameSpan.end,
    name_utf16_start: nameNode
      ? nameNode.getStart(sourceFile)
      : node.getStart(sourceFile),
    top_level: topLevel,
    scope,
    node,
  };
  definitions.push(definition);
  definitionByName.set(qualifiedName, definition);
}

function addReexport(name, node, moduleSpecifier, topLevel) {
  const nodeSpan = span(node);
  reexports.push({
    name,
    kind: "reexport",
    category: "other",
    start: nodeSpan.start,
    end: nodeSpan.end,
    name_start: nodeSpan.start,
    name_end: nodeSpan.end,
    name_utf16_start: node.getStart(sourceFile),
    top_level: topLevel,
    scope: [],
    module_specifier: moduleSpecifier,
  });
}

function visitChildren(node, scope, topLevel) {
  ts.forEachChild(node, (child) => visit(child, scope, topLevel, node));
}

function visitFunctionBody(node, scope) {
  visitChildren(node, scope, false);
}

function visit(node, scope, topLevel, parent) {
  if (ts.isFunctionDeclaration(node)) {
    const name = nameOf(node.name) || (node.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.DefaultKeyword) ? "default" : undefined);
    addDefinition(name, node, node.name, classifyFunction(name || "", false, node), "function", scope, topLevel);
    visitFunctionBody(node, name ? [...scope, name] : scope);
    return;
  }

  if (ts.isClassDeclaration(node)) {
    const name = nameOf(node.name) || (node.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.DefaultKeyword) ? "default" : undefined);
    addDefinition(name, node, node.name, "class", "other", scope, topLevel);
    visitChildren(node, name ? [...scope, name] : scope, false);
    return;
  }

  if (ts.isVariableStatement(node)) {
    const isConst = (node.declarationList.flags & ts.NodeFlags.Const) !== 0;
    for (const declaration of node.declarationList.declarations) {
      const initializer = declaration.initializer;
      const entries = declarationNames(declaration.name);
      for (const entry of entries) {
        const classification = variableKind(entry.name, initializer, isConst);
        const kind = classification.kind;
        const category = classification.category;
        const symbolNode = node.declarationList.declarations.length === 1 ? node : declaration;
        addDefinition(entry.name, symbolNode, entry.node, kind, category, scope, topLevel);
      }
      // Visit the initializer once per declaration, not once per binding: a
      // destructuring pattern has no single owner, so nesting under each binding
      // would duplicate every inner symbol. Single binding still nests by name.
      if (initializer) {
        const initializerScope = entries.length === 1 ? [...scope, entries[0].name] : scope;
        visit(initializer, initializerScope, false, declaration);
      }
      if (declaration.type) visit(declaration.type, scope, false, declaration);
    }
    return;
  }

  if (ts.isMethodDeclaration(node) || ts.isGetAccessorDeclaration(node) || ts.isSetAccessorDeclaration(node)) {
    const name = nameOf(node.name);
    const isObjectMethod = parent && ts.isObjectLiteralExpression(parent);
    addDefinition(name, node, node.name, isObjectMethod ? "object_method" : "method", "function", scope, false);
    visitFunctionBody(node, name ? [...scope, name] : scope);
    return;
  }

  if (ts.isFunctionExpression(node)) {
    const name = nameOf(node.name);
    if (name) addDefinition(name, node, node.name, classifyFunction(name, false, node), "function", scope, false);
    visitFunctionBody(node, name ? [...scope, name] : scope);
    return;
  }

  if (ts.isArrowFunction(node)) {
    visitFunctionBody(node, scope);
    return;
  }

  if (ts.isPropertyAssignment(node) && node.initializer && (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer))) {
    const name = nameOf(node.name);
    addDefinition(name, node, node.name, ts.isArrowFunction(node.initializer) ? (containsJsx(node.initializer) ? "react_component" : "arrow_function") : "object_method", "function", scope, false);
    visit(node.initializer, name ? [...scope, name] : scope, false, node);
    return;
  }

  if (ts.isInterfaceDeclaration(node)) {
    addDefinition(nameOf(node.name), node, node.name, "interface", "type", scope, topLevel);
    visitChildren(node, scope, false);
    return;
  }

  if (ts.isTypeAliasDeclaration(node)) {
    addDefinition(nameOf(node.name), node, node.name, "type", "type", scope, topLevel);
    visitChildren(node, scope, false);
    return;
  }

  if (ts.isEnumDeclaration(node)) {
    const name = nameOf(node.name);
    addDefinition(name, node, node.name, "enum", "type", scope, topLevel);
    const memberScope = name ? [...scope, name] : scope;
    for (const member of node.members) {
      addDefinition(
        nameOf(member.name),
        member,
        member.name,
        "enum_member",
        "constant",
        memberScope,
        false,
      );
    }
    return;
  }

  if (ts.isModuleDeclaration(node)) {
    const name = nameOf(node.name);
    addDefinition(name, node, node.name, "namespace", "other", scope, topLevel);
    visitChildren(node, name ? [...scope, name] : scope, false);
    return;
  }

  if (ts.isExportAssignment(node) && !node.isExportEquals) {
    const expression = node.expression;
    if (ts.isArrowFunction(expression) || ts.isFunctionExpression(expression) || ts.isClassExpression(expression)) {
      const kind = ts.isClassExpression(expression)
        ? "class"
        : classifyFunction("default", ts.isArrowFunction(expression), expression);
      const category = ts.isClassExpression(expression) ? "other" : "function";
      addDefinition("default", node, undefined, kind, category, scope, topLevel);
    }
    visit(expression, [...scope, "default"], false, node);
    return;
  }

  // Re-exports carry no local binding, so the branches above never match them.
  // Emit a pseudo-symbol per re-export so barrel files aren't reported as empty.
  // Only forms with a module specifier (`... from '...'`); a bare
  // `export { local }` re-exports an already-collected local declaration.
  if (ts.isExportDeclaration(node) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) {
    const moduleSpecifier = node.moduleSpecifier.text;
    const clause = node.exportClause;
    if (!clause) {
      // export * from './x'
      addReexport("*", node, moduleSpecifier, topLevel);
    } else if (ts.isNamespaceExport(clause)) {
      // export * as ns from './x'
      addReexport(nameOf(clause.name) || "*", node, moduleSpecifier, topLevel);
    } else if (ts.isNamedExports(clause)) {
      // export { a, b as c, default as X } from './x'
      for (const element of clause.elements) {
        const name = nameOf(element.name);
        if (name) addReexport(name, node, moduleSpecifier, topLevel);
      }
    }
    return;
  }

  visitChildren(node, scope, topLevel);
}

if (sourceFile) visitChildren(sourceFile, [], true);

function definitionForReference(name, scope) {
  for (let length = scope.length; length >= 0; length -= 1) {
    const qualified = [...scope.slice(0, length), name].join(".");
    if (definitionByName.has(qualified)) return definitionByName.get(qualified);
  }
  return undefined;
}

function definitionScopes(definition) {
  return [definition.name.split("."), definition.scope];
}

function bindingNames(node, output) {
  if (!node) return;
  if (ts.isIdentifier(node)) {
    output.add(node.text);
    return;
  }
  if (ts.isObjectBindingPattern(node) || ts.isArrayBindingPattern(node)) {
    for (const element of node.elements) {
      if (!ts.isOmittedExpression(element)) bindingNames(element.name, output);
    }
  }
}

function bindingsIn(node) {
  const bindings = new Set();
  function walk(current) {
    if (ts.isParameter(current)) bindingNames(current.name, bindings);
    if (ts.isVariableDeclaration(current)) bindingNames(current.name, bindings);
    if (ts.isCatchClause(current)) bindingNames(current.variableDeclaration?.name, bindings);
    ts.forEachChild(current, walk);
  }
  walk(node);
  return bindings;
}

function propertyAccessName(node) {
  if (!ts.isPropertyAccessExpression(node)) return undefined;
  const left = ts.isIdentifier(node.expression)
    ? node.expression.text
    : propertyAccessName(node.expression);
  return left ? `${left}.${node.name.text}` : undefined;
}

function dependenciesFor(definition) {
  const dependencies = new Set();
  const bindings = bindingsIn(definition.node);
  const targetParts = definition.name.split(".");
  const targetSimpleName = targetParts[targetParts.length - 1];
  const declarationRanges = new Set(definitions.map((item) => `${item.name_start}:${item.name_end}`));

  function recordIdentifier(identifier) {
    const name = identifier.text;
    if (name === targetSimpleName || declarationRanges.has(`${byteOffset(identifier.getStart(sourceFile))}:${byteOffset(identifier.getEnd())}`)) return;
    const resolved = definitionForReference(name, definitionScopes(definition));
    const ownPrefix = `${definition.name}.`;
    if (bindings.has(name) && (!resolved || !resolved.name.startsWith(ownPrefix))) return;
    if (resolved && resolved.name !== definition.name) {
      dependencies.add(resolved.name);
    }
  }

  function walk(node) {
    if (ts.isPropertyAccessExpression(node)) {
      const qualified = propertyAccessName(node);
      const resolved = qualified ? definitionForReference(qualified, definitionScopes(definition)) : undefined;
      if (resolved && resolved.name !== definition.name) dependencies.add(resolved.name);
      walk(node.expression);
      return;
    }
    if (ts.isIdentifier(node)) {
      recordIdentifier(node);
      return;
    }
    ts.forEachChild(node, walk);
  }

  walk(definition.node);
  return [...dependencies];
}

function scriptKindForFile(fileName) {
  const extension = path.extname(fileName).slice(1).toLowerCase();
  return {
    ts: ts.ScriptKind.TS,
    tsx: ts.ScriptKind.TSX,
    js: ts.ScriptKind.JS,
    jsx: ts.ScriptKind.JSX,
  }[extension];
}

function isSupportedSource(fileName) {
  return scriptKindForFile(fileName) !== undefined
    && !fileName.split(path.sep).includes("node_modules");
}

function projectLanguageService() {
  const defaultOptions = {
    allowJs: true,
    checkJs: false,
    jsx: ts.JsxEmit.Preserve,
    module: ts.ModuleKind.NodeNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
    noEmit: true,
    skipLibCheck: true,
    target: ts.ScriptTarget.Latest,
  };
  let compilerOptions = defaultOptions;
  let rootNames = [];
  const configPath = ts.findConfigFile(
    workspaceRoot,
    ts.sys.fileExists,
    "tsconfig.json",
  );
  if (configPath) {
    const config = ts.readConfigFile(configPath, ts.sys.readFile);
    if (!config.error) {
      const parsed = ts.parseJsonConfigFileContent(
        config.config,
        ts.sys,
        path.dirname(configPath),
      );
      compilerOptions = { ...defaultOptions, ...parsed.options, allowJs: true };
      rootNames = parsed.fileNames;
    }
  }

  const sourceTexts = new Map();
  if (!isWorkspaceOperation) sourceTexts.set(currentFileName, request.source);
  const scriptFileNames = new Set(rootNames.map((fileName) => path.resolve(fileName)));
  if (!isWorkspaceOperation) scriptFileNames.add(currentFileName);

  function sourceTextFor(fileName) {
    const normalized = path.resolve(fileName);
    if (sourceTexts.has(normalized)) return sourceTexts.get(normalized);
    const source = ts.sys.readFile(normalized);
    if (source !== undefined) sourceTexts.set(normalized, source);
    return source;
  }

  function addImportedFiles(fileName, visited) {
    const normalized = path.resolve(fileName);
    if (visited.has(normalized)) return;
    visited.add(normalized);
    if (!isSupportedSource(normalized)) return;
    const source = sourceTextFor(normalized);
    if (source === undefined) return;
    scriptFileNames.add(normalized);
    const importedModules = [];
    const importedFile = ts.createSourceFile(
      normalized,
      source,
      ts.ScriptTarget.Latest,
      true,
      scriptKindForFile(normalized),
    );
    function visitImports(node) {
      if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
        importedModules.push(node.moduleSpecifier.text);
      } else if (ts.isExportDeclaration(node) && node.moduleSpecifier && ts.isStringLiteral(node.moduleSpecifier)) {
        importedModules.push(node.moduleSpecifier.text);
      } else if (ts.isImportEqualsDeclaration(node) && ts.isExternalModuleReference(node.moduleReference)
        && ts.isStringLiteral(node.moduleReference.expression)) {
        importedModules.push(node.moduleReference.expression.text);
      } else if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)
        && node.expression.text === "require" && node.arguments.length === 1
        && ts.isStringLiteral(node.arguments[0])) {
        importedModules.push(node.arguments[0].text);
      }
      ts.forEachChild(node, visitImports);
    }
    visitImports(importedFile);
    for (const moduleName of importedModules) {
      const resolved = ts.resolveModuleName(
        moduleName,
        normalized,
        compilerOptions,
        ts.sys,
      ).resolvedModule;
      if (resolved?.resolvedFileName) addImportedFiles(resolved.resolvedFileName, visited);
    }
  }

  if (isWorkspaceOperation) {
    if (scriptFileNames.size === 0) {
      for (const fileName of ts.sys.readDirectory(
        workspaceRoot,
        [".ts", ".tsx", ".js", ".jsx"],
        undefined,
        undefined,
      )) {
        if (isSupportedSource(fileName)) scriptFileNames.add(path.resolve(fileName));
      }
    }
  } else {
    addImportedFiles(currentFileName, new Set());
  }

  const sourceFileCache = new Map();
  function sourceFileFor(fileName) {
    const normalized = path.resolve(fileName);
    if (sourceFileCache.has(normalized)) return sourceFileCache.get(normalized);
    const source = sourceTextFor(normalized);
    if (source === undefined) return undefined;
    const parsed = ts.createSourceFile(
      normalized,
      source,
      ts.ScriptTarget.Latest,
      true,
      scriptKindForFile(normalized),
    );
    sourceFileCache.set(normalized, parsed);
    return parsed;
  }

  const host = {
    fileExists: ts.sys.fileExists,
    getCompilationSettings: () => compilerOptions,
    getCurrentDirectory: () => workspaceRoot,
    getDefaultLibFileName: (options) => ts.getDefaultLibFilePath(options),
    getScriptFileNames: () => [...scriptFileNames],
    getScriptKind: scriptKindForFile,
    getScriptSnapshot: (fileName) => {
      const source = sourceTextFor(fileName);
      return source === undefined ? undefined : ts.ScriptSnapshot.fromString(source);
    },
    getScriptVersion: () => "1",
    readDirectory: ts.sys.readDirectory,
    readFile: ts.sys.readFile,
    useCaseSensitiveFileNames: () => ts.sys.useCaseSensitiveFileNames,
  };
  return {
    service: ts.createLanguageService(host, ts.createDocumentRegistry()),
    sourceFileFor,
  };
}

function targetPosition(service, symbol) {
  const definition = definitionByName.get(symbol);
  if (definition) return definition.name_utf16_start;
  let position;
  function visit(node) {
    if (position !== undefined) return;
    if (propertyAccessName(node) === symbol) {
      position = node.name.getStart(sourceFile);
      return;
    }
    if (ts.isIdentifier(node) && node.text === symbol) {
      position = node.getStart(sourceFile);
      return;
    }
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
  if (position === undefined) return undefined;
  return position;
}

function locationForSpan(fileName, textSpan, symbol, isDefinition) {
  const file = path.resolve(fileName);
  const fileSource = projectSourceFile(file);
  if (!fileSource) return undefined;
  const start = fileSource.getLineAndCharacterOfPosition(textSpan.start);
  const end = fileSource.getLineAndCharacterOfPosition(textSpan.start + textSpan.length);
  return {
    file,
    symbol,
    start_line: start.line + 1,
    end_line: end.line + 1,
    start_column: start.character + 1,
    end_column: end.character + 1,
    is_definition: isDefinition,
  };
}

let projectSourceFile;

function isCallReference(fileName, position) {
  const file = projectSourceFile(fileName);
  if (!file) return false;
  let child = ts.getTokenAtPosition(file, position);
  while (child && child.parent) {
    const parent = child.parent;
    if ((ts.isCallExpression(parent) || ts.isNewExpression(parent))
      && parent.expression === child) return true;
    if (ts.isPropertyAccessExpression(parent) || ts.isElementAccessExpression(parent)) {
      child = parent;
      continue;
    }
    return false;
  }
  return false;
}

// Rendering a component (`<Foo/>`) is the JSX equivalent of calling it.
function isJsxReference(fileName, position) {
  const file = projectSourceFile(fileName);
  if (!file) return false;
  let child = ts.getTokenAtPosition(file, position);
  while (child && child.parent) {
    const parent = child.parent;
    if ((ts.isJsxOpeningElement(parent) || ts.isJsxSelfClosingElement(parent))
      && parent.tagName === child) return true;
    if (ts.isPropertyAccessExpression(parent)) {
      child = parent;
      continue;
    }
    return false;
  }
  return false;
}

// A component is often exported through a trivial wrapper
// (`const Foo = memo(FooComponent)`); usages of the wrapper binding are usages
// of the component. Returns the wrapper binding's name position, if any.
function wrapperBindingPosition(sourceFile, targetName) {
  let position;
  function isWrapperCallee(expression) {
    const name = ts.isIdentifier(expression)
      ? expression.text
      : (ts.isPropertyAccessExpression(expression) ? expression.name.text : undefined);
    return name === "memo" || name === "forwardRef";
  }
  function visit(node) {
    if (position !== undefined) return;
    if (ts.isVariableDeclaration(node) && node.name && ts.isIdentifier(node.name)
      && node.initializer && ts.isCallExpression(node.initializer)
      && isWrapperCallee(node.initializer.expression)
      && node.initializer.arguments.length >= 1) {
      const argument = node.initializer.arguments[0];
      if (ts.isIdentifier(argument) && argument.text === targetName) {
        position = node.name.getStart(sourceFile);
        return;
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
  return position;
}

function isDefinitionReference(service, reference) {
  const definitionsAtReference = service.getDefinitionAtPosition(
    reference.fileName,
    reference.textSpan.start,
  ) || [];
  return definitionsAtReference.some((definition) => (
    path.resolve(definition.fileName) === path.resolve(reference.fileName)
      && definition.textSpan.start === reference.textSpan.start
  ));
}

function callableName(node) {
  if (node.name) return nameOf(node.name) || "anonymous";
  const parent = node.parent;
  if (parent && ts.isVariableDeclaration(parent) && ts.isIdentifier(parent.name)) return parent.name.text;
  if (parent && ts.isPropertyAssignment(parent)) return nameOf(parent.name) || "anonymous";
  return "anonymous";
}

function callerAt(fileName, position) {
  const file = projectSourceFile(fileName);
  if (!file) return "<module>";
  let node = ts.getTokenAtPosition(file, position);
  while (node && node !== file) {
    if (ts.isFunctionLike(node)) return callableName(node);
    node = node.parent;
  }
  return "<module>";
}

function searchDefinitionsInFile(file) {
  const results = [];

  function visitChildrenLocal(node, scope) {
    ts.forEachChild(node, (child) => visit(child, scope, node));
  }

  function add(name, node, nameNode, kind, scope) {
    if (!name || !nameNode) return;
    results.push({
      name: [...scope, name].join("."),
      kind,
      nameStart: nameNode.getStart(file),
      node,
    });
  }

  function visit(node, scope, parent) {
    if (ts.isFunctionDeclaration(node)) {
      const name = nameOf(node.name) || (node.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.DefaultKeyword) ? "default" : undefined);
      if (name) add(name, node, node.name, classifyFunction(name, false, node), scope);
      visitChildrenLocal(node, name ? [...scope, name] : scope);
      return;
    }
    if (ts.isClassDeclaration(node)) {
      const name = nameOf(node.name) || (node.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.DefaultKeyword) ? "default" : undefined);
      if (name) add(name, node, node.name, "class", scope);
      visitChildrenLocal(node, name ? [...scope, name] : scope);
      return;
    }
    if (ts.isVariableStatement(node)) {
      const isConst = (node.declarationList.flags & ts.NodeFlags.Const) !== 0;
      for (const declaration of node.declarationList.declarations) {
        const entries = declarationNames(declaration.name);
        for (const entry of entries) {
          const classification = variableKind(entry.name, declaration.initializer, isConst);
          add(entry.name, declaration, entry.node, classification.kind, scope);
        }
        // Visit the initializer once per declaration (see the parse walker):
        // a destructuring pattern nests inner symbols in the enclosing scope
        // rather than duplicating them under every binding.
        if (declaration.initializer) {
          const initializerScope = entries.length === 1 ? [...scope, entries[0].name] : scope;
          visit(declaration.initializer, initializerScope, declaration);
        }
      }
      return;
    }
    if (ts.isMethodDeclaration(node) || ts.isGetAccessorDeclaration(node) || ts.isSetAccessorDeclaration(node)) {
      const name = nameOf(node.name);
      if (name) add(name, node, node.name, "method", scope);
      visitChildrenLocal(node, name ? [...scope, name] : scope);
      return;
    }
    if (ts.isPropertyAssignment(node) && node.initializer
      && (ts.isArrowFunction(node.initializer) || ts.isFunctionExpression(node.initializer))) {
      const name = nameOf(node.name);
      if (name) add(name, node, node.name, "object_method", scope);
      visit(node.initializer, name ? [...scope, name] : scope, node);
      return;
    }
    if (ts.isInterfaceDeclaration(node)) {
      add(nameOf(node.name), node, node.name, "interface", scope);
      return;
    }
    if (ts.isTypeAliasDeclaration(node)) {
      add(nameOf(node.name), node, node.name, "type", scope);
      return;
    }
    if (ts.isEnumDeclaration(node)) {
      const name = nameOf(node.name);
      add(name, node, node.name, "enum", scope);
      const memberScope = name ? [...scope, name] : scope;
      for (const member of node.members) {
        add(nameOf(member.name), member, member.name, "enum_member", memberScope);
      }
      return;
    }
    if (ts.isModuleDeclaration(node)) {
      const name = nameOf(node.name);
      if (name) add(name, node, node.name, "namespace", scope);
      visitChildrenLocal(node, name ? [...scope, name] : scope);
      return;
    }
    ts.forEachChild(node, (child) => visit(child, scope, node));
  }

  visit(file, [], undefined);
  return results;
}

function searchOutput(service) {
  const query = String(request.query || "").toLowerCase();
  const kind = request.kind;
  const maxResults = Math.min(Math.max(Number(request.max_results || 200), 1), 1000);
  const offset = navigationOffset();
  const matches = [];
  let skipped = 0;
  const files = (service.getProgram()?.getSourceFiles() || [])
    .filter((file) => isSupportedSource(file.fileName))
    .map((file) => ({ file, normalized: path.resolve(file.fileName) }))
    .filter(({ normalized }) => (
      normalized === workspaceRoot || normalized.startsWith(`${workspaceRoot}${path.sep}`)
    ))
    .sort((left, right) => compareText(left.normalized, right.normalized));
  for (const { file, normalized } of files) {
    const definitions = searchDefinitionsInFile(file).sort((left, right) => (
      compareNumber(left.nameStart, right.nameStart)
        || compareText(left.name, right.name)
        || compareText(left.kind, right.kind)
    ));
    for (const definition of definitions) {
      if (query && !definition.name.toLowerCase().includes(query)) continue;
      if (kind && definition.kind !== kind) continue;
      if (skipped < offset) {
        skipped += 1;
        continue;
      }
      const start = file.getLineAndCharacterOfPosition(definition.nameStart);
      const nameEnd = definition.nameStart + definition.name.split(".").pop().length;
      const end = file.getLineAndCharacterOfPosition(nameEnd);
      matches.push({
        name: definition.name,
        kind: definition.kind,
        file: normalized,
        start_line: start.line + 1,
        end_line: end.line + 1,
        start_column: start.character + 1,
        end_column: end.character + 1,
      });
      if (matches.length > maxResults) {
        return { search_symbols: matches.slice(0, maxResults), truncated: true };
      }
    }
  }
  return { search_symbols: matches, truncated: false };
}

function navigationMaxResults() {
  return Math.min(Math.max(Number(request.max_results || 200), 1), 1000);
}

function navigationOffset() {
  const offset = Number(request.offset || 0);
  if (!Number.isFinite(offset)) return 0;
  return Math.min(Math.max(Math.trunc(offset), 0), Number.MAX_SAFE_INTEGER);
}

function compareText(left, right) {
  const a = String(left || "");
  const b = String(right || "");
  return a < b ? -1 : a > b ? 1 : 0;
}

function compareNumber(left, right) {
  return Number(left || 0) - Number(right || 0);
}

function compareLocations(left, right) {
  return compareText(left.file, right.file)
    || compareNumber(left.start_line, right.start_line)
    || compareNumber(left.start_column, right.start_column)
    || compareNumber(left.end_line, right.end_line)
    || compareNumber(left.end_column, right.end_column)
    || compareText(left.symbol, right.symbol)
    || compareNumber(Boolean(left.is_definition), Boolean(right.is_definition));
}

function compareCallers(left, right) {
  return compareLocations(left, right) || compareText(left.caller, right.caller);
}

function compareCallees(left, right) {
  return compareLocations(left.location, right.location)
    || compareText(left.callee, right.callee)
    || compareLocations(left.definition || {}, right.definition || {});
}

function compareDiagnostics(left, right) {
  return compareNumber(left.start, right.start)
    || compareNumber(left.length, right.length)
    || compareNumber(left.code, right.code)
    || compareNumber(left.category, right.category)
    || compareText(
      ts.flattenDiagnosticMessageText(left.messageText, "\\n"),
      ts.flattenDiagnosticMessageText(right.messageText, "\\n"),
    );
}

function limitedResults(items, comparator) {
  const maxResults = navigationMaxResults();
  const offset = navigationOffset();
  if (comparator && (offset > 0 || items.length > maxResults)) {
    items.sort(comparator);
  }
  const page = items.slice(offset, offset + maxResults + 1);
  return {
    items: page.slice(0, maxResults),
    truncated: page.length > maxResults,
  };
}

function definitionEntryAtPosition(service, position) {
  return service.getDefinitionAtPosition(currentFileName, position)?.[0];
}

function sourceFileFromProgram(service, fileName) {
  return service.getProgram()?.getSourceFile(path.resolve(fileName));
}

function declarationContainerAt(file, position) {
  let node = ts.getTokenAtPosition(file, position);
  while (node && node !== file) {
    if (ts.isVariableDeclaration(node)
      && node.initializer
      && (ts.isFunctionLike(node.initializer) || ts.isClassExpression(node.initializer))) {
      return node.initializer;
    }
    if (ts.isFunctionLike(node) || ts.isClassDeclaration(node) || ts.isClassExpression(node)) return node;
    node = node.parent;
  }
  return undefined;
}

function containerNameNode(container) {
  if (container.name) return container.name;
  if (container.parent && ts.isVariableDeclaration(container.parent)) {
    return container.parent.name;
  }
  return undefined;
}

function containerLocation(container) {
  const source = container.getSourceFile();
  const nameNode = containerNameNode(container);
  const symbol = nameOf(nameNode) || "<module>";
  const start = nameNode ? nameNode.getStart(source) : container.getStart(source);
  const end = nameNode ? nameNode.getEnd() : container.getStart(source);
  return locationForSpan(source.fileName, { start, length: end - start }, symbol, true);
}

function definitionAnchorForContainer(container) {
  const source = container.getSourceFile();
  const nameNode = containerNameNode(container);
  const name = nameOf(nameNode);
  if (!nameNode || !name) return undefined;
  const start = nameNode.getStart(source);
  return {
    fileName: source.fileName,
    textSpan: { start, length: nameNode.getEnd() - start },
    name,
  };
}

function callableDefinitionAnchorForLocation(service, location) {
  const file = sourceFileFromProgram(service, location.file);
  const position = file
    ? positionForLineColumn(file, location.start_line, location.start_column)
    : undefined;
  const container = position === undefined ? undefined : declarationContainerAt(file, position);
  const anchor = container ? definitionAnchorForContainer(container) : undefined;
  // A parameter or destructured local binding can resolve inside an enclosing
  // function, but it does not own that function's body. Expand only when the
  // discovered symbol is the declaration name of the callable container itself.
  return anchor && anchor.textSpan.start === position ? anchor : undefined;
}

function nodeId(location) {
  return `${path.resolve(location.file)}:${location.start_line}:${location.start_column}`;
}

function resolveAliasedSymbol(checker, symbol) {
  if (!symbol) return undefined;
  let resolved = symbol;
  if ((resolved.flags & ts.SymbolFlags.Alias) !== 0) resolved = checker.getAliasedSymbol(resolved);
  return resolved;
}

function projectDefinitionForResolvedSymbol(program, resolved) {
  if (!resolved) return { location: undefined, knownExternal: false };
  const declarations = resolved.declarations || [];
  const declaration = declarations.find((item) => {
    const source = item.getSourceFile();
    return isSupportedSource(source.fileName)
      && !program.isSourceFileDefaultLibrary(source)
      && !program.isSourceFileFromExternalLibrary(source);
  });
  if (!declaration) {
    return { location: undefined, knownExternal: declarations.length > 0 };
  }
  const source = declaration.getSourceFile();
  const nameNode = declaration.name;
  const start = nameNode ? nameNode.getStart(source) : declaration.getStart(source);
  const end = nameNode ? nameNode.getEnd() : declaration.getStart(source);
  return {
    location: locationForSpan(
      source.fileName,
      { start, length: Math.max(0, end - start) },
      resolved.getName(),
      true,
    ),
    knownExternal: false,
  };
}

function staticCalleeName(expression) {
  if (ts.isIdentifier(expression) || ts.isPrivateIdentifier(expression)) return expression.text;
  if (expression.kind === ts.SyntaxKind.ThisKeyword) return "this";
  if (ts.isPropertyAccessExpression(expression)) {
    const owner = staticCalleeName(expression.expression);
    return owner ? `${owner}.${expression.name.text}` : expression.name.text;
  }
  if (ts.isElementAccessExpression(expression)
    && (ts.isStringLiteral(expression.argumentExpression)
      || ts.isNumericLiteral(expression.argumentExpression))) {
    const owner = staticCalleeName(expression.expression);
    return owner ? `${owner}.${expression.argumentExpression.text}` : expression.argumentExpression.text;
  }
  return undefined;
}

function projectCalleeLocations(service, definition) {
  const program = service.getProgram();
  const checker = program?.getTypeChecker();
  const file = sourceFileFromProgram(service, definition.fileName);
  if (!checker || !file) return [];
  const container = declarationContainerAt(file, definition.textSpan.start);
  if (!container) return [];
  const callees = [];
  const seen = new Set();
  function visit(node) {
    if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
      const expression = node.expression;
      const lookupNode = ts.isPropertyAccessExpression(expression) || ts.isElementAccessExpression(expression)
        ? expression.name || expression.argumentExpression
        : expression;
      const symbol = lookupNode ? checker.getSymbolAtLocation(lookupNode) : undefined;
      const resolvedSymbol = resolveAliasedSymbol(checker, symbol);
      const { location: definitionLocation, knownExternal } = projectDefinitionForResolvedSymbol(
        program,
        resolvedSymbol,
      );
      const callee = definitionLocation?.symbol || (!knownExternal ? staticCalleeName(expression) : undefined);
      if (callee) {
        const callStart = expression.getStart(file);
        const callEnd = expression.getEnd();
        const callLocation = locationForSpan(
          file.fileName,
          { start: callStart, length: callEnd - callStart },
          callee,
          false,
        );
        if (callLocation) {
          const targetKey = definitionLocation
            ? `${definitionLocation.file}:${definitionLocation.start_line}`
            : "unresolved";
          const key = `${callLocation.file}:${callLocation.start_line}:${callLocation.start_column}:${targetKey}`;
          if (!seen.has(key)) {
            seen.add(key);
            callees.push({
              callee,
              location: callLocation,
              definition: definitionLocation || null,
            });
          }
        }
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(container);
  return callees;
}

function callerLocationAt(service, fileName, position) {
  const file = sourceFileFromProgram(service, fileName);
  if (!file) return undefined;
  const container = declarationContainerAt(file, position);
  if (!container) return undefined;
  const location = containerLocation(container);
  return location
    ? { location, definition: definitionAnchorForContainer(container) }
    : undefined;
}

function implementationsOutput(service, position) {
  const implementations = service.getImplementationAtPosition(currentFileName, position) || [];
  const locations = implementations
    .filter((implementation) => isSupportedSource(implementation.fileName))
    .map((implementation) => {
      const sourceFile = sourceFileFromProgram(service, implementation.fileName);
      const spanName = sourceFile?.text
        .slice(
          implementation.textSpan.start,
          implementation.textSpan.start + implementation.textSpan.length,
        )
        .trim();
      return locationForSpan(
        implementation.fileName,
        implementation.textSpan,
        implementation.name || spanName || "implementation",
        true,
      );
    })
    .filter(Boolean);
  const limited = limitedResults(locations, compareLocations);
  return {
    implementations: limited.items,
    truncated: limited.truncated,
    symbol_found: true,
  };
}

function positionForLineColumn(file, line, column) {
  const lineStarts = file.getLineStarts();
  const lineIndex = Math.max(0, line - 1);
  const character = Math.max(0, column - 1);
  if (lineIndex >= lineStarts.length) return undefined;
  const position = lineStarts[lineIndex] + character;
  const lineEnd = lineIndex + 1 < lineStarts.length ? lineStarts[lineIndex + 1] : file.text.length;
  return position <= lineEnd ? position : undefined;
}

function typeInfoOutput(service) {
  const file = sourceFileFromProgram(service, currentFileName);
  if (!file) return { type_info: null };
  const position = positionForLineColumn(file, Math.max(1, request.line || 1), Math.max(1, request.column || 1));
  if (position === undefined) return { type_info: null };
  const info = service.getQuickInfoAtPosition(currentFileName, position);
  if (!info) return { type_info: null };
  const location = locationForSpan(
    currentFileName,
    info.textSpan,
    ts.displayPartsToString(info.displayParts || []),
    false,
  );
  return {
    type_info: {
      kind: info.kind || "unknown",
      display: ts.displayPartsToString(info.displayParts || []),
      documentation: ts.displayPartsToString(info.documentation || []),
      location,
    },
  };
}

function diagnosticSeverity(category) {
  return {
    [ts.DiagnosticCategory.Error]: "error",
    [ts.DiagnosticCategory.Warning]: "warning",
    [ts.DiagnosticCategory.Suggestion]: "suggestion",
    [ts.DiagnosticCategory.Message]: "message",
  }[category] || "message";
}

function diagnosticsOutput(service) {
  const file = sourceFileFromProgram(service, currentFileName);
  if (!file) return { diagnostics: [], truncated: false };
  const diagnostics = [
    ...service.getSyntacticDiagnostics(currentFileName),
    ...service.getSemanticDiagnostics(currentFileName),
  ];
  let symbolSpan;
  if (request.symbol) {
    const position = targetPosition(service, request.symbol);
    const definition = position === undefined ? undefined : definitionEntryAtPosition(service, position);
    if (!definition) return { diagnostics: [], truncated: false, symbol_found: false };
    const definitionFile = sourceFileFromProgram(service, definition.fileName);
    const container = definitionFile
      ? declarationContainerAt(definitionFile, definition.textSpan.start)
      : undefined;
    symbolSpan = container
      ? { start: container.getStart(definitionFile), length: container.getEnd() - container.getStart(definitionFile) }
      : definition.textSpan;
  }
  const matching = diagnostics.filter((diagnostic) => {
    if (!symbolSpan || diagnostic.start === undefined) return true;
    const end = diagnostic.start + (diagnostic.length || 0);
    return end >= symbolSpan.start && diagnostic.start <= symbolSpan.start + symbolSpan.length;
  });
  const limited = limitedResults(matching, compareDiagnostics);
  return {
    diagnostics: limited.items
      .map((diagnostic) => {
        const start = diagnostic.start || 0;
        const end = start + (diagnostic.length || 0);
        const startLocation = file.getLineAndCharacterOfPosition(start);
        const endLocation = file.getLineAndCharacterOfPosition(end);
        return {
          severity: diagnosticSeverity(diagnostic.category),
          code: Number(diagnostic.code),
          message: ts.flattenDiagnosticMessageText(diagnostic.messageText, "\\n"),
          start_line: startLocation.line + 1,
          end_line: endLocation.line + 1,
          start_column: startLocation.character + 1,
          end_column: endLocation.character + 1,
        };
      }),
    truncated: limited.truncated,
    symbol_found: true,
  };
}

function hierarchyOutput(service, position) {
  const rootDefinition = definitionEntryAtPosition(service, position);
  if (!rootDefinition) return { hierarchy_nodes: [], hierarchy_edges: [], symbol_found: false };
  const rootLocation = locationForSpan(
    rootDefinition.fileName,
    rootDefinition.textSpan,
    rootDefinition.name,
    true,
  );
  if (!rootLocation) return { hierarchy_nodes: [], hierarchy_edges: [], symbol_found: false };
  const maxDepth = Math.min(Math.max(Number(request.depth || 2), 1), 8);
  // Which edges to expand: "callees" or "callers" cut one side; anything else
  // (including the default) traverses both, matching the historical behavior.
  const direction = request.direction === "callees" || request.direction === "callers"
    ? request.direction
    : "both";
  // Keep a single call always consumable: cap total nodes and stop expanding
  // high-fan-in "hub" symbols (useTheme, formatters, …) whose caller subtree
  // would otherwise drag in most of the codebase.
  const NODE_BUDGET = 120;
  const HUB_CALLER_LIMIT = 40;
  const nodes = new Map();
  const edges = new Map();
  let truncated = false;
  const queue = [{ location: rootLocation, definition: rootDefinition, depth: 0 }];
  function addNode(location) {
    const id = nodeId(location);
    if (!nodes.has(id)) {
      if (nodes.size >= NODE_BUDGET) {
        truncated = true;
        return null;
      }
      nodes.set(id, {
        id,
        symbol: location.symbol,
        file: location.file,
        start_line: location.start_line,
        end_line: location.end_line,
        hub: false,
        callers_elided: 0,
      });
    }
    return id;
  }
  addNode(rootLocation);
  const visited = new Set([nodeId(rootLocation)]);
  while (queue.length > 0) {
    const current = queue.shift();
    if (current.depth >= maxDepth) continue;
    if (direction !== "callers") {
      const callees = projectCalleeLocations(service, current.definition);
      for (const callee of callees) {
        const target = callee.definition;
        if (!target) continue;
        const caller = addNode(current.location);
        const calleeId = addNode(target);
        if (caller === null || calleeId === null) continue;
        edges.set(`${caller}->${calleeId}`, { caller, callee: calleeId });
        if (!visited.has(calleeId)) {
          visited.add(calleeId);
          // Queue expansion must be anchored at the discovered callee itself.
          // Reusing `current.definition` here would assign the parent's body to
          // a different graph node and manufacture unrelated edges.
          const nextDefinition = callableDefinitionAnchorForLocation(service, target);
          if (nextDefinition) {
            queue.push({ location: target, definition: nextDefinition, depth: current.depth + 1 });
          }
        }
      }
    }
    if (direction !== "callees") {
      const references = collectCallerReferences(
        service,
        current.location.symbol,
        current.definition.fileName,
        current.definition.textSpan.start,
      );
      const callSites = references.filter((reference) =>
        isSupportedSource(reference.fileName)
        && (isCallReference(reference.fileName, reference.textSpan.start)
          || isJsxReference(reference.fileName, reference.textSpan.start)));
      // Hub guard: never expand the callers of a high-fan-in symbol (except the
      // explicitly queried root). Keep the node, flag it, drop its caller subtree.
      if (current.depth >= 1 && callSites.length > HUB_CALLER_LIMIT) {
        const node = nodes.get(nodeId(current.location));
        if (node) {
          node.hub = true;
          node.callers_elided = callSites.length;
        }
        truncated = true;
        continue;
      }
      for (const reference of callSites) {
        const callerNode = callerLocationAt(service, reference.fileName, reference.textSpan.start);
        if (!callerNode) continue;
        const caller = callerNode.location;
        const callerId = addNode(caller);
        const callee = addNode(current.location);
        if (callerId === null || callee === null) continue;
        edges.set(`${callerId}->${callee}`, { caller: callerId, callee });
        if (!visited.has(callerId)) {
          visited.add(callerId);
          // The reference position resolves to the callee, not to its enclosing
          // caller. `callerNode.definition` is deliberately derived from the
          // caller's declaration container instead.
          if (callerNode.definition) {
            queue.push({
              location: caller,
              definition: callerNode.definition,
              depth: current.depth + 1,
            });
          }
        }
      }
    }
  }
  return {
    hierarchy_nodes: [...nodes.values()],
    hierarchy_edges: [...edges.values()],
    truncated,
    symbol_found: true,
  };
}

function navigationOutput() {
  const language = projectLanguageService();
  projectSourceFile = language.sourceFileFor;
  const service = language.service;
  if (request.operation === "search_symbols") return searchOutput(service);
  const symbol = request.symbol;
  if (request.operation === "get_diagnostics") return diagnosticsOutput(service);
  if (request.operation === "get_type") return typeInfoOutput(service);
  if (request.operation === "go_to_definition") {
    const current = projectSourceFile(currentFileName);
    const position = positionForLineColumn(
      current,
      Math.max(1, request.line || 1),
      Math.max(1, request.column || 1),
    );
    if (position === undefined) return { definition: null };
    const definition = service.getDefinitionAtPosition(currentFileName, position)?.[0];
    return {
      definition: definition
        ? locationForSpan(definition.fileName, definition.textSpan, definition.name, true)
        : null,
    };
  }

  const position = targetPosition(service, symbol);
  if (position === undefined) {
    if (request.operation === "find_callers") return { callers: [], symbol_found: false };
    if (request.operation === "find_callees") return { callees: [], symbol_found: false };
    if (request.operation === "get_call_hierarchy") return { hierarchy_nodes: [], hierarchy_edges: [], symbol_found: false };
    if (request.operation === "find_implementations") return { implementations: [], symbol_found: false };
    return { references: [], symbol_found: false };
  }
  if (request.operation === "find_implementations") return implementationsOutput(service, position);
  if (request.operation === "find_callees") {
    const definition = definitionEntryAtPosition(service, position);
    const callees = definition ? projectCalleeLocations(service, definition) : [];
    const limited = limitedResults(callees, compareCallees);
    return {
      callees: limited.items,
      truncated: limited.truncated,
      symbol_found: true,
    };
  }
  if (request.operation === "get_call_hierarchy") return hierarchyOutput(service, position);
  const references = service.getReferencesAtPosition(currentFileName, position) || [];
  if (request.operation === "find_references") {
    const locations = references
      .filter((reference) => isSupportedSource(reference.fileName))
      .map((reference) => locationForSpan(
        reference.fileName,
        reference.textSpan,
        symbol,
        isDefinitionReference(service, reference),
      ))
      .filter(Boolean);
    const limited = limitedResults(locations, compareLocations);
    return {
      references: limited.items,
      truncated: limited.truncated,
      symbol_found: true,
    };
  }
  const callers = collectCallerReferences(service, symbol, currentFileName, position)
      .filter((reference) => isSupportedSource(reference.fileName))
      .filter((reference) =>
        isCallReference(reference.fileName, reference.textSpan.start)
        || isJsxReference(reference.fileName, reference.textSpan.start))
      .map((reference) => {
        const location = locationForSpan(
          reference.fileName,
          reference.textSpan,
          symbol,
          false,
        );
        return location
          ? { ...location, caller: callerAt(reference.fileName, reference.textSpan.start) }
          : undefined;
      })
      .filter(Boolean);
  const limited = limitedResults(callers, compareCallers);
  return {
    callers: limited.items,
    truncated: limited.truncated,
    symbol_found: true,
  };
}

// References that count as callers of `symbol`: its own references plus those of
// a trivial wrapper binding (`const Foo = memo(FooComponent)`), deduplicated.
// Works for any file so the call hierarchy resolves callers the same way
// `find_callers` does, including memo/forwardRef-wrapped JSX components.
function collectCallerReferences(service, symbol, fileName, position) {
  const base = service.getReferencesAtPosition(fileName, position) || [];
  const source = projectSourceFile(fileName);
  const targetLeaf = symbol.split(".").pop();
  const wrapperPosition = source ? wrapperBindingPosition(source, targetLeaf) : undefined;
  if (wrapperPosition === undefined) return base;
  const wrapperReferences = service.getReferencesAtPosition(fileName, wrapperPosition) || [];
  const seen = new Set();
  const combined = [];
  for (const reference of [...base, ...wrapperReferences]) {
    const key = `${reference.fileName}:${reference.textSpan.start}`;
    if (seen.has(key)) continue;
    seen.add(key);
    combined.push(reference);
  }
  return combined;
}

const output = {
  symbols: [...definitions.map(({ node, ...definition }) => definition), ...reexports],
  dependencies: Object.fromEntries(definitions.map((definition) => [definition.name, dependenciesFor(definition)])),
};
if (request.operation !== "parse") Object.assign(output, navigationOutput());
process.stdout.write(JSON.stringify(output));
