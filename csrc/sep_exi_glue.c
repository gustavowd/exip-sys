#include <stdio.h>
#include <string.h>
#include <stddef.h>
#include "procTypes.h"
#include "EXIParser.h"
#include "contentHandler.h"
#include "stringManipulate.h"

/* Precisa ficar idêntico ao SepSink em src/lib.rs */
typedef struct sep_sink {
    void* user;
    int (*start_element)(void*, const char* uri, size_t ulen, const char* name, size_t nlen);
    int (*end_element)(void*);
    int (*attribute)(void*, const char* uri, size_t ulen, const char* name, size_t nlen);
    int (*text)(void*, const char* s, size_t n);
    int (*binary)(void*, const unsigned char* b, size_t n);
} sep_sink;

extern const EXIPSchema* sep_get_schema(void);

#define SINK(d) ((sep_sink*)(d))
#define RC(x)   ((x) ? EXIP_HANDLER_STOP : EXIP_OK)

static errorCode h_fatal(const errorCode code, const char* msg, void* d) { return EXIP_HANDLER_STOP; }

static errorCode h_start(QName q, void* d) {
    const String* u = q.uri;
    return RC(SINK(d)->start_element(SINK(d)->user,
              (u && u->length) ? u->str : "", u ? u->length : 0,
              q.localName->str, q.localName->length));
}
static errorCode h_end(void* d) { return RC(SINK(d)->end_element(SINK(d)->user)); }
static errorCode h_attr(QName q, void* d) {
    const String* u = q.uri;
    return RC(SINK(d)->attribute(SINK(d)->user,
              (u && u->length) ? u->str : "", u ? u->length : 0,
              q.localName->str, q.localName->length));
}
static errorCode emit(void* d, const char* t, size_t n) { return RC(SINK(d)->text(SINK(d)->user, t, n)); }

static errorCode h_string(const String v, void* d) { return emit(d, v.str, v.length); }
static errorCode h_int(Integer v, void* d) {
    char b[32]; int n = snprintf(b, sizeof b, "%lld", (long long) v); return emit(d, b, (size_t) n);
}
static errorCode h_bool(bool v, void* d) { return v ? emit(d, "true", 4) : emit(d, "false", 5); }
static errorCode h_float(Float v, void* d) {
    char b[48]; int n = snprintf(b, sizeof b, "%lldE%d", (long long) v.mantissa, (int) v.exponent);
    return emit(d, b, (size_t) n);
}
static errorCode h_decimal(Decimal v, void* d) { return h_float(v, d); }
static errorCode h_binary(const char* b, Index n, void* d) {
    return RC(SINK(d)->binary(SINK(d)->user, (const unsigned char*) b, (size_t) n));
}
static errorCode h_qname(const QName q, void* d) {          /* valor de xsi:type */
    char b[512];
    int n = snprintf(b, sizeof b, "%.*s:%.*s",
                     q.uri ? (int) q.uri->length : 0, q.uri ? q.uri->str : "",
                     (int) q.localName->length, q.localName->str);
    return emit(d, b, (size_t) n);
}
static errorCode h_unsupported_dt(EXIPDateTime dt, void* d) { return EXIP_HANDLER_STOP; }

/* retorna 0 se OK, senão o código de erro da EXIP */
int sep_exi_decode(char* data, size_t len, const sep_sink* sink_in, int out_of_band)
{
    Parser parser;
    BinaryBuffer buffer;
    EXIOptions ops;
    sep_sink sink = *sink_in;
    errorCode err;

    buffer.buf = data;
    buffer.bufLen = len;
    buffer.bufContent = len;
    buffer.ioStrm.readWriteToStream = NULL;
    buffer.ioStrm.stream = NULL;

    err = parse.initParser(&parser, buffer, &sink);
    if (err != EXIP_OK) return (int) err;

    if (out_of_band) {
        makeDefaultOpts(&ops);
        parser.strm.header.opts = ops;
    }

    parser.handler.fatalError   = h_fatal;
    parser.handler.error        = h_fatal;
    parser.handler.startElement = h_start;
    parser.handler.endElement   = h_end;
    parser.handler.attribute    = h_attr;
    parser.handler.stringData   = h_string;
    parser.handler.intData      = h_int;
    parser.handler.booleanData  = h_bool;
    parser.handler.floatData    = h_float;
    parser.handler.decimalData  = h_decimal;
    parser.handler.binaryData   = h_binary;
    parser.handler.qnameData    = h_qname;
    parser.handler.dateTimeData = h_unsupported_dt;

    err = parse.parseHeader(&parser, out_of_band ? true : false);
    if (err == EXIP_OK) err = parse.setSchema(&parser, (EXIPSchema*) sep_get_schema());
    while (err == EXIP_OK) err = parse.parseNext(&parser);

    parse.destroyParser(&parser);
    return err == EXIP_PARSING_COMPLETE ? 0 : (int) err;
}
