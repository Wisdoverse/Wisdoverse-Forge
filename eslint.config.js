import eslint from '@eslint/js'
import tseslint from 'typescript-eslint'
import importPlugin from 'eslint-plugin-import'
import prettier from 'eslint-config-prettier'

const nodeScriptGlobals = {
  console: 'readonly',
  process: 'readonly',
}

const browserScriptGlobals = {
  ...nodeScriptGlobals,
  document: 'readonly',
  getComputedStyle: 'readonly',
  localStorage: 'readonly',
  window: 'readonly',
}

export default tseslint.config(
  {
    ignores: [
      'dist/**',
      'node_modules/**',
      'coverage/**',
      '*.config.js',
      '*.config.ts',
      'hooks/**',
      'sidecar/**',
      'shared/generated/platform/**',
      '.worktrees/**',
      'docs/**',
      'docker/**',
      'public/**',
      '__mocks__/**',
      'config/**',
      'dist-*/**',
      '.tmp/**',
      'tests/**',
      'rust/**',
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  ...tseslint.configs.stylistic,
  {
    plugins: {
      import: importPlugin,
    },
    settings: {
      'import/resolver': {
        typescript: {
          alwaysTryTypes: true,
          project: './tsconfig.json',
        },
      },
    },
    rules: {
      'import/order': [
        'error',
        {
          groups: ['builtin', 'external', 'internal', ['parent', 'sibling'], 'index', 'type'],
          'newlines-between': 'never',
          alphabetize: {
            order: 'asc',
            caseInsensitive: true,
          },
        },
      ],
      'import/no-duplicates': 'error',
      'import/no-unresolved': 'off',
      'import/first': 'error',
      'import/newline-after-import': 'error',
    },
  },
  {
    files: ['**/*.ts', '**/*.tsx'],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: {
        project: './tsconfig.eslint.json',
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      '@typescript-eslint/explicit-function-return-type': [
        'error',
        {
          allowExpressions: true,
          allowTypedFunctionExpressions: true,
          allowHigherOrderFunctions: true,
        },
      ],
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
      '@typescript-eslint/no-non-null-assertion': 'error',
      '@typescript-eslint/prefer-nullish-coalescing': 'error',
      '@typescript-eslint/prefer-optional-chain': 'error',
      '@typescript-eslint/strict-boolean-expressions': 'off',
      '@typescript-eslint/consistent-type-imports': [
        'error',
        {
          prefer: 'type-imports',
          fixStyle: 'inline-type-imports',
        },
      ],
      '@typescript-eslint/consistent-type-definitions': ['error', 'interface'],
      '@typescript-eslint/naming-convention': [
        'error',
        { selector: 'interface', format: ['PascalCase'] },
        { selector: 'typeAlias', format: ['PascalCase'] },
        { selector: 'enum', format: ['PascalCase'] },
        { selector: 'enumMember', format: ['UPPER_CASE', 'PascalCase'] },
      ],
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/await-thenable': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/require-await': 'error',
      'no-console': ['error', { allow: ['warn', 'error', 'info'] }],
      'no-debugger': 'error',
      'no-alert': 'error',
      'no-var': 'error',
      'prefer-const': 'error',
      'prefer-template': 'error',
      'prefer-arrow-callback': 'error',
      'arrow-body-style': ['error', 'as-needed'],
      'object-shorthand': 'error',
      'no-param-reassign': ['error', { props: false }],
      'no-nested-ternary': 'error',
      'no-unneeded-ternary': 'error',
      eqeqeq: ['error', 'always', { null: 'ignore' }],
      curly: ['error', 'all'],
      'default-case': 'error',
      'default-case-last': 'error',
      'no-fallthrough': 'error',
      'no-case-declarations': 'error',
      'no-return-await': 'off',
      '@typescript-eslint/return-await': ['error', 'in-try-catch'],
      'no-throw-literal': 'off',
      '@typescript-eslint/only-throw-error': 'error',
      'max-depth': ['error', 4],
      'max-lines-per-function': [
        'error',
        {
          max: 100,
          skipBlankLines: true,
          skipComments: true,
        },
      ],
      complexity: ['error', 15],
      'no-implicit-coercion': ['error', { allow: ['!!'] }],
      'no-lonely-if': 'error',
      'no-else-return': ['error', { allowElseIf: false }],
      'no-useless-return': 'error',
      'no-useless-concat': 'error',
      'no-useless-computed-key': 'error',
      'no-useless-rename': 'error',
      'prefer-destructuring': [
        'error',
        {
          array: false,
          object: true,
        },
      ],
    },
  },
  {
    files: ['vscode-extension/**/*.ts'],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: {
        project: './vscode-extension/tsconfig.json',
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  {
    files: ['src/app/**/*.{ts,tsx}'],
    rules: {
      '@typescript-eslint/array-type': 'off',
      '@typescript-eslint/consistent-type-definitions': 'off',
      '@typescript-eslint/explicit-function-return-type': 'off',
      '@typescript-eslint/no-misused-promises': 'off',
      '@typescript-eslint/only-throw-error': 'off',
      '@typescript-eslint/prefer-nullish-coalescing': 'off',
      complexity: 'off',
      'default-case': 'off',
      'import/order': 'off',
      'max-lines-per-function': 'off',
      'no-alert': 'off',
      'no-else-return': 'off',
      'no-nested-ternary': 'off',
      'prefer-destructuring': 'off',
    },
  },
  {
    files: ['scripts/**/*.cjs'],
    languageOptions: {
      sourceType: 'commonjs',
      globals: {
        ...nodeScriptGlobals,
        module: 'readonly',
        require: 'readonly',
      },
    },
    rules: {
      '@typescript-eslint/no-require-imports': 'off',
      'no-console': 'off',
    },
  },
  {
    files: ['scripts/**/*.js', 'scripts/**/*.mjs'],
    languageOptions: {
      globals: browserScriptGlobals,
    },
    rules: {
      'no-console': 'off',
    },
  },
  {
    files: ['**/*.test.ts', '**/*.spec.ts', '**/__tests__/**/*.ts'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-non-null-assertion': 'off',
      'max-lines-per-function': 'off',
      'no-console': 'off',
    },
  },
  prettier
)
