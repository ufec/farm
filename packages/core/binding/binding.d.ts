/* tslint:disable */
/* eslint-disable */

/* auto-generated by NAPI-RS */

/** Resolve hook filters, works as `||`. If any importers or sources matches any regex item in the Vec, we treat it as filtered. */
export interface JsPluginResolveHookFilters {
  importers: Array<string>;
  sources: Array<string>;
}
export interface JsPluginLoadHookFilters {
  ids: Array<string>;
}
export interface JsPluginTransformHookFilters {
  ids: Array<string>;
}
export interface JsUpdateResult {}
export type JsCompiler = Compiler;
export class Compiler {
  constructor(config: object);
  /**
   * async compile, return promise
   *
   * TODO: usage example
   */
  compile(): Promise<void>;
  /** sync compile */
  compileSync(): void;
  /**
   * async update, return promise
   *
   * TODO: usage example
   */
  update(paths: Array<string>): Promise<JsUpdateResult>;
  /** sync update */
  updateSync(paths: Array<string>): JsUpdateResult;
}
