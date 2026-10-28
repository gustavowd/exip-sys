#include <ctype.h>
#include <errno.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "procTypes.h"
#include "EXISerializer.h"
#include "stringManipulate.h"

extern const EXIPSchema* sep_get_schema(void);

/* Códigos do glue (negativos); os positivos são da EXIP */
#define SEP_ERR_UNSUPPORTED (-1000)  /* float/decimal/dateTime/list/qname */
#define SEP_ERR_BAD_VALUE   (-1001)
#define SEP_ERR_NOMEM       (-1002)
#define SEP_ERR_STATE       (-1003)  /* texto em elemento sem conteúdo simples */

#define CHK(x)  do { errorCode r_ = (x); if (r_ != EXIP_OK) return (int) r_; } while (0)
#define CHKI(x) do { int r_ = (x); if (r_ != 0) return r_; } while (0)

typedef struct { unsigned char* data; size_t len; size_t cap; } outbuf;

typedef struct sep_enc {
    EXIStream strm;
    BinaryBuffer buffer;
    char chunk[512];
    outbuf out;
    EXITypeClass content_type;
    int open;
} sep_enc;

void sep_enc_abort(sep_enc* e);

static size_t out_write(void* buf, size_t size, void* stream)
{
    outbuf* o = (outbuf*) stream;
    if (o->len + size > o->cap) {
        size_t nc = o->cap ? o->cap : 1024;
        unsigned char* p;
        while (nc < o->len + size) nc *= 2;
        p = (unsigned char*) realloc(o->data, nc);
        if (!p) return 0;
        o->data = p;
        o->cap = nc;
    }
    memcpy(o->data + o->len, buf, size);
    o->len += size;
    return size;
}

/* Copia s[0..n) para a memList do stream (vive até closeEXIStream) */
static int mkstr(sep_enc* e, const char* s, size_t n, String* out)
{
    char* tmp;
    errorCode r;
    if (n == 0) { out->str = NULL; out->length = 0; return 0; }
    tmp = (char*) malloc(n + 1);
    if (!tmp) return SEP_ERR_NOMEM;
    memcpy(tmp, s, n);
    tmp[n] = '\0';
    r = asciiToString(tmp, out, &e->strm.memList, true);
    free(tmp);
    return r == EXIP_OK ? 0 : (int) r;
}

static int hexval(char c)
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static int put_value(sep_enc* e, EXITypeClass t, const char* v, size_t n)
{
    if (t != VALUE_TYPE_STRING_CLASS) {        /* tipos não-string: colapsa espaços */
        while (n && isspace((unsigned char) *v)) { v++; n--; }
        while (n && isspace((unsigned char) v[n - 1])) n--;
    }
    switch (t) {
    case VALUE_TYPE_STRING_CLASS: {
        String s;
        CHKI(mkstr(e, v, n, &s));
        CHK(serialize.stringData(&e->strm, s));
        return 0;
    }
    case VALUE_TYPE_BOOLEAN_CLASS: {
        bool b;
        if ((n == 4 && !memcmp(v, "true", 4)) || (n == 1 && v[0] == '1')) b = true;
        else if ((n == 5 && !memcmp(v, "false", 5)) || (n == 1 && v[0] == '0')) b = false;
        else return SEP_ERR_BAD_VALUE;
        CHK(serialize.booleanData(&e->strm, b));
        return 0;
    }
    case VALUE_TYPE_INTEGER_CLASS: {
        char tmp[48];
        char* end;
        long long x;
        if (n == 0 || n >= sizeof tmp) return SEP_ERR_BAD_VALUE;
        memcpy(tmp, v, n);
        tmp[n] = '\0';
        errno = 0;
        x = strtoll(tmp, &end, 10);
        if (errno != 0 || *end != '\0') return SEP_ERR_BAD_VALUE;   /* > INT64_MAX também cai aqui */
        CHK(serialize.intData(&e->strm, (Integer) x));
        return 0;
    }
    case VALUE_TYPE_BINARY_CLASS: {          /* xs:hexBinary */
        size_t i, m = n / 2;
        unsigned char* b;
        errorCode r;
        if (n % 2) return SEP_ERR_BAD_VALUE;
        b = (unsigned char*) malloc(m ? m : 1);
        if (!b) return SEP_ERR_NOMEM;
        for (i = 0; i < m; i++) {
            int hi = hexval(v[2 * i]), lo = hexval(v[2 * i + 1]);
            if (hi < 0 || lo < 0) { free(b); return SEP_ERR_BAD_VALUE; }
            b[i] = (unsigned char) ((hi << 4) | lo);
        }
        r = serialize.binaryData(&e->strm, (const char*) b, (Index) m);
        free(b);
        return r == EXIP_OK ? 0 : (int) r;
    }
    default:
        return SEP_ERR_UNSUPPORTED;
    }
}

sep_enc* sep_enc_open(void)
{
    sep_enc* e = (sep_enc*) calloc(1, sizeof *e);
    if (!e) return NULL;

    e->buffer.buf = e->chunk;
    e->buffer.bufLen = sizeof e->chunk;
    e->buffer.bufContent = 0;
    e->buffer.ioStrm.readWriteToStream = out_write;
    e->buffer.ioStrm.stream = &e->out;

    serialize.initHeader(&e->strm);     /* opções padrão: sem cookie, sem options no header */
    if (serialize.initStream(&e->strm, e->buffer, (EXIPSchema*) sep_get_schema()) != EXIP_OK) {
        free(e);
        return NULL;
    }
    e->open = 1;
    if (serialize.exiHeader(&e->strm) != EXIP_OK || serialize.startDocument(&e->strm) != EXIP_OK) {
        sep_enc_abort(e);
        return NULL;
    }
    return e;
}

int sep_enc_start_element(sep_enc* e, const char* uri, size_t ulen, const char* name, size_t nlen)
{
    String u, l;
    QName q = {&u, &l, NULL};
    EXITypeClass vt = VALUE_TYPE_NONE_CLASS;
    CHKI(mkstr(e, uri, ulen, &u));
    CHKI(mkstr(e, name, nlen, &l));
    CHK(serialize.startElement(&e->strm, q, &vt));
    e->content_type = vt;
    return 0;
}

int sep_enc_attribute(sep_enc* e, const char* uri, size_t ulen, const char* name, size_t nlen,
                      const char* val, size_t vlen)
{
    String u, l;
    QName q = {&u, &l, NULL};
    EXITypeClass vt = VALUE_TYPE_NONE_CLASS;
    CHKI(mkstr(e, uri, ulen, &u));
    CHKI(mkstr(e, name, nlen, &l));
    CHK(serialize.attribute(&e->strm, q, true, &vt));
    return put_value(e, vt, val, vlen);
}

int sep_enc_text(sep_enc* e, const char* t, size_t n)
{
    EXITypeClass vt = e->content_type;
    if (vt == VALUE_TYPE_NONE_CLASS) return SEP_ERR_STATE;
    e->content_type = VALUE_TYPE_NONE_CLASS;
    return put_value(e, vt, t, n);
}

int sep_enc_end_element(sep_enc* e)
{
    e->content_type = VALUE_TYPE_NONE_CLASS;
    CHK(serialize.endElement(&e->strm));
    return 0;
}

/* Sempre libera o encoder (sucesso ou erro). Em sucesso, *out é de quem chamou: libere com sep_enc_free_buf. */
int sep_enc_finish(sep_enc* e, unsigned char** out, size_t* out_len)
{
    errorCode r = serialize.endDocument(&e->strm);
    errorCode c = serialize.closeEXIStream(&e->strm);
    e->open = 0;
    if (r != EXIP_OK || c != EXIP_OK) {
        int code = (int) (r != EXIP_OK ? r : c);
        sep_enc_abort(e);
        return code;
    }
    *out = e->out.data;
    *out_len = e->out.len;
    e->out.data = NULL;
    free(e);
    return 0;
}

void sep_enc_abort(sep_enc* e)
{
    if (!e) return;
    if (e->open) serialize.closeEXIStream(&e->strm);
    free(e->out.data);
    free(e);
}

void sep_enc_free_buf(unsigned char* p) { free(p); }
