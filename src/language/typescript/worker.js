const fs = require("node:fs");
const ts = require("typescript");

const request = JSON.parse(fs.readFileSync(0, "utf8"));
const scriptKind = {
  ts: ts.ScriptKind.TS,
  tsx: ts.ScriptKind.TSX,
  js: ts.ScriptKind.JS,
  jsx: ts.ScriptKind.JSX,
}[request.extension];

if (scriptKind === undefined) {
  process.stderr.write(`unsupported extension: ${request.extension}`);
  process.exit(2);
}

const sourceFile = ts.createSourceFile(
  request.path,
  request.source,
  ts.ScriptTarget.Latest,
  true,
  scriptKind,
);

if (sourceFile.parseDiagnostics.length > 0) {
  const message = sourceFile.parseDiagnostics
    .map((diagnostic) => ts.flattenDiagnosticMessageText(diagnostic.messageText, " "))
    .join("; ");
  process.stderr.write(message);
  process.exit(3);
}

const definitions = [];
const definitionByName = new Map();

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
    top_level: topLevel,
    scope,
    node,
  };
  definitions.push(definition);
  definitionByName.set(qualifiedName, definition);
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
      for (const entry of declarationNames(declaration.name)) {
        const initializer = declaration.initializer;
        const classification = variableKind(entry.name, initializer, isConst);
        const kind = classification.kind;
        const category = classification.category;
        const symbolNode = node.declarationList.declarations.length === 1 ? node : declaration;
        addDefinition(entry.name, symbolNode, entry.node, kind, category, scope, topLevel);
        if (initializer) visit(initializer, [...scope, entry.name], false, declaration);
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
    addDefinition(nameOf(node.name), node, node.name, "enum", "type", scope, topLevel);
    visitChildren(node, scope, false);
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

  visitChildren(node, scope, topLevel);
}

visitChildren(sourceFile, [], true);

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
  const left = ts.isIdentifier(node.expression) ? node.expression.text : undefined;
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

const output = {
  symbols: definitions.map(({ node, ...definition }) => definition),
  dependencies: Object.fromEntries(definitions.map((definition) => [definition.name, dependenciesFor(definition)])),
};
process.stdout.write(JSON.stringify(output));
