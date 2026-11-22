import { defineConfig } from 'oxlint';
import solidV2 from 'eslint-plugin-solid/configs/v2';

export default defineConfig({
  jsPlugins: ['eslint-plugin-solid'],
  ignorePatterns: ['**/*.gen.*', 'dist'],
  settings: solidV2.settings,
  rules: {
    ...solidV2.rules,
    // `translator` reads the dictionary accessor each time a message is
    // looked up, so what it is given is tracked wherever `t()` is called.
    'solid/reactivity': ['warn', { customReactiveFunctions: ['translator'] }],
  },
});
