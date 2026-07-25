/* Table-driven Anubis token scanner — independent architecture (not a port of
 * backend_c/anubis_sh_parse.c). Emits one token class name per line for a
 * minimal SH surface used by author-diversity checks.
 *
 * Token classes: IDENT KEYWORD INT STRING OP LPAREN RPAREN LBRACE RBRACE
 * COMMA SEMI COLON EQ ARROW OTHER EOF
 */
#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int is_kw(const char *s, size_t n) {
  static const char *kw[] = {
      "fn",   "let",  "mut",  "if",    "else",  "while", "for",  "return",
      "true", "false","uses", "struct","enum",  "impl",  "match","in",
      NULL};
  for (int i = 0; kw[i]; i++) {
    if (strlen(kw[i]) == n && memcmp(kw[i], s, n) == 0)
      return 1;
  }
  return 0;
}

static void emit(const char *cls) { puts(cls); }

int main(int argc, char **argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: token_scan <file.anb>\n");
    return 2;
  }
  FILE *f = fopen(argv[1], "rb");
  if (!f) {
    perror(argv[1]);
    return 1;
  }
  fseek(f, 0, SEEK_END);
  long n = ftell(f);
  fseek(f, 0, SEEK_SET);
  char *buf = (char *)malloc((size_t)n + 1);
  if (!buf) {
    fclose(f);
    return 1;
  }
  if (fread(buf, 1, (size_t)n, f) != (size_t)n) {
    free(buf);
    fclose(f);
    return 1;
  }
  buf[n] = 0;
  fclose(f);

  const char *p = buf;
  while (*p) {
    if (isspace((unsigned char)*p)) {
      p++;
      continue;
    }
    if (p[0] == '/' && p[1] == '/') {
      while (*p && *p != '\n')
        p++;
      continue;
    }
    if (p[0] == '/' && p[1] == '*') {
      p += 2;
      while (*p && !(p[0] == '*' && p[1] == '/'))
        p++;
      if (*p)
        p += 2;
      continue;
    }
    if (isalpha((unsigned char)*p) || *p == '_') {
      const char *s = p;
      while (isalnum((unsigned char)*p) || *p == '_')
        p++;
      emit(is_kw(s, (size_t)(p - s)) ? "KEYWORD" : "IDENT");
      continue;
    }
    if (isdigit((unsigned char)*p)) {
      while (isdigit((unsigned char)*p))
        p++;
      emit("INT");
      continue;
    }
    if (*p == '"') {
      p++;
      while (*p && *p != '"') {
        if (*p == '\\' && p[1])
          p += 2;
        else
          p++;
      }
      if (*p == '"')
        p++;
      emit("STRING");
      continue;
    }
    if (p[0] == '-' && p[1] == '>') {
      p += 2;
      emit("ARROW");
      continue;
    }
    if (*p == '(') {
      p++;
      emit("LPAREN");
      continue;
    }
    if (*p == ')') {
      p++;
      emit("RPAREN");
      continue;
    }
    if (*p == '{') {
      p++;
      emit("LBRACE");
      continue;
    }
    if (*p == '}') {
      p++;
      emit("RBRACE");
      continue;
    }
    if (*p == ',') {
      p++;
      emit("COMMA");
      continue;
    }
    if (*p == ';') {
      p++;
      emit("SEMI");
      continue;
    }
    if (*p == ':') {
      p++;
      emit("COLON");
      continue;
    }
    if (*p == '=') {
      p++;
      emit("EQ");
      continue;
    }
    p++;
    emit("OP");
  }
  emit("EOF");
  free(buf);
  return 0;
}
