// Checks that a built wasm module can be started the way wasm-bindgen starts it.
//
// wasm-bindgen's glue grows the `__wbindgen_externrefs` table by four entries as
// the app comes up. A build that makes that table a fixed size dies there:
//
//     Uncaught (in promise) RangeError: WebAssembly.Table.grow(): failed to grow
//     table by 4
//         at __wbindgen_init_externref_table (...)
//
// and the page then sits on its loading screen, with nothing in the console to
// say why. Trunk's default wasm-opt (binaryen 105, well before wasm-bindgen's
// externref layout) did exactly that to the table, which is why `Trunk.toml`
// pins a current binaryen and why the release build in CI runs this check.
//
// Usage: node scripts/check-wasm-table.mjs dist/violetta_lie_detector_sim-*_bg.wasm

import { readFileSync } from "node:fs";

const files = process.argv.slice(2);
if (files.length === 0) {
  console.error("usage: node scripts/check-wasm-table.mjs <wasm file>...");
  process.exit(2);
}

let failed = false;

for (const file of files) {
  const module = new WebAssembly.Module(readFileSync(file));

  // wasm-bindgen's glue only imports functions, and which ones they are does not
  // matter here: nothing is called, the module is only instantiated.
  const imports = {};
  for (const imported of WebAssembly.Module.imports(module)) {
    imports[imported.module] ??= {};
    imports[imported.module][imported.name] = () => 0;
  }

  const { exports } = new WebAssembly.Instance(module, imports);
  const table = exports.__wbindgen_externrefs;
  if (!table) {
    console.error(`${file}: no __wbindgen_externrefs table in the exports`);
    failed = true;
    continue;
  }

  // This is what __wbindgen_init_externref_table does, verbatim.
  try {
    const offset = table.grow(4);
    table.set(0, undefined);
    table.set(offset + 0, undefined);
    table.set(offset + 1, null);
    table.set(offset + 2, true);
    table.set(offset + 3, false);
  } catch (error) {
    console.error(`${file}: ${error.message}`);
    console.error(
      "The page would stand still on its loading screen. Check the wasm-opt version in Trunk.toml.",
    );
    failed = true;
    continue;
  }

  console.log(`${file}: externref table grows to ${table.length}`);
}

process.exit(failed ? 1 : 0);
