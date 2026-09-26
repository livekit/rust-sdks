import { defineConfig } from 'tsup';

export default defineConfig({
  entry: ['src/index.ts'],
  format: ['cjs', 'esm'],
  target: 'node18',
  dts: true,
  clean: true,

  // @ubjs/node loads a native N-API addon and @ubjs/core is the shared runtime;
  // both stay external so they resolve from node_modules at runtime.
  external: ['@ubjs/node', '@ubjs/core'],

  // ref: https://stackoverflow.com/a/75868407
  shims: true,
});
