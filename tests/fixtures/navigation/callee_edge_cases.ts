import {
  canonicalTarget as aliasedTarget,
  Service as AliasedService,
} from "./callee_targets";

declare const dynamicApi: any;
declare function getFactory(): () => void;

export function inspectCalls() {
  aliasedTarget();
  aliasedTarget();
  new AliasedService();
  new MissingConstructor();
  dynamicApi["perform"]();
  dynamicApi?.optional?.();
  getFactory()();
  Promise.resolve();
  (() => 1)();
}

export class Runner {
  known() {}

  run() {
    this.known();
    this.missing();
  }
}
