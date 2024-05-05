/**
 * binding.cjs and binding.d.ts are generated by the Rust. Please do not modify them manually.
 * If you want to modify the binding, please modify the Rust code or manually wrap the generated code in index.js or index.d.ts.
 */
import binding from './binding.cjs';
import bindingPath from './resolve-binding.cjs';
process.env.FARM_LIB_CORE_PATH = bindingPath;

const Compiler = binding.Compiler;
const JsFileWatcher = binding.JsFileWatcher;
export { Compiler, bindingPath, JsFileWatcher };
