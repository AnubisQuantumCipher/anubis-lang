// tree-sitter grammar for Anubis (.anb) — highlight-oriented, not the parser of record.
// LANGUAGE.md remains authoritative. Must `tree-sitter generate` cleanly.

module.exports = grammar({
  name: 'anubis',

  extras: $ => [
    /\s/,
    $.line_comment,
    $.block_comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    // keep empty: resolve with precedence instead
  ],

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.function_item,
      $.import_item,
      $.struct_item,
      $.enum_item,
      $.impl_item,
      $.module_item,
    ),

    import_item: $ => seq('import', $.path, optional(';')),

    function_item: $ => seq(
      optional('pub'),
      'fn',
      field('name', $.identifier),
      $.parameters,
      optional(seq('->', $._type)),
      repeat($.contract_clause),
      $.block,
    ),

    contract_clause: $ => seq(
      choice('requires', 'ensures', 'uses'),
      '(',
      optional($._expression),
      ')',
    ),

    parameters: $ => seq(
      '(',
      optional(seq($.parameter, repeat(seq(',', $.parameter)))),
      ')',
    ),
    parameter: $ => seq($.identifier, optional(seq(':', $._type))),

    struct_item: $ => seq(
      'struct',
      $.identifier,
      '{',
      repeat($.field_decl),
      '}',
    ),
    field_decl: $ => seq($.identifier, ':', $._type, optional(',')),

    enum_item: $ => seq(
      'enum',
      $.identifier,
      '{',
      optional(seq($.identifier, repeat(seq(',', $.identifier)), optional(','))),
      '}',
    ),
    impl_item: $ => seq('impl', $.identifier, $.block),
    module_item: $ => seq('module', $.identifier, $.block),

    _type: $ => choice(
      $.identifier,
      seq($.identifier, '<', $._type, '>'),
    ),

    block: $ => seq('{', repeat($._statement), '}'),

    _statement: $ => choice(
      $.let_statement,
      $.if_statement,
      $.while_statement,
      $.return_statement,
      $.expression_statement,
    ),

    let_statement: $ => seq(
      'let',
      optional('mut'),
      $.identifier,
      optional(seq(':', $._type)),
      '=',
      $._expression,
      ';',
    ),

    if_statement: $ => prec.right(seq(
      'if',
      field('condition', $._expression),
      $.block,
      optional(seq('else', choice($.if_statement, $.block))),
    )),

    while_statement: $ => seq(
      'while',
      field('condition', $._expression),
      $.block,
    ),

    // Semicolon required — removes return/expr ambiguity for highlight grammar.
    return_statement: $ => seq(
      'return',
      optional(field('value', $._expression)),
      ';',
    ),

    expression_statement: $ => seq($._expression, ';'),

    _expression: $ => choice(
      $.binary_expression,
      $.call_expression,
      $.parenthesized_expression,
      $.identifier,
      $.number,
      $.string,
      $.boolean,
    ),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    call_expression: $ => prec(10, seq(
      field('function', $.identifier),
      '(',
      optional(seq($._expression, repeat(seq(',', $._expression)))),
      ')',
    )),

    binary_expression: $ => {
      const table = [
        ['||', 1],
        ['&&', 2],
        ['==', 3],
        ['!=', 3],
        ['<', 4],
        ['>', 4],
        ['<=', 4],
        ['>=', 4],
        ['+', 5],
        ['-', 5],
        ['*', 6],
        ['/', 6],
        ['%', 6],
      ];
      return choice(...table.map(([op, precedence]) =>
        prec.left(precedence, seq(
          field('left', $._expression),
          field('operator', op),
          field('right', $._expression),
        ))
      ));
    },

    path: $ => seq($.identifier, repeat(seq('.', $.identifier))),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,
    number: $ => /\d+(\.\d+)?/,
    string: $ => /"([^"\\]|\\.)*"/,
    boolean: $ => choice('true', 'false'),

    line_comment: $ => token(seq('//', /.*/)),
    block_comment: $ => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),
  },
});
