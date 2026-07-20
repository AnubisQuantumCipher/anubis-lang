(line_comment) @comment
(block_comment) @comment
(string) @string
(number) @number
(boolean) @constant.builtin
(identifier) @variable
(function_item name: (identifier) @function)
(call_expression function: (identifier) @function.call)

"fn" @keyword.function
"let" @keyword
"mut" @keyword
"if" @keyword.control
"else" @keyword.control
"while" @keyword.control
"return" @keyword.control
"import" @keyword.import
"pub" @keyword.modifier
"struct" @keyword
"enum" @keyword
"impl" @keyword
"module" @keyword
"requires" @keyword
"ensures" @keyword
"uses" @keyword
