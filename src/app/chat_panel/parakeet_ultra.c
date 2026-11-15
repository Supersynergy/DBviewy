// Small command-line adapter for the transcribe.cpp 0.4.2 C API.
// argv[1] is the GGUF checkpoint, argv[2] is mono 16 kHz float32 PCM.
#include "transcribe.h"
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    FILE *file = fopen(argv[2], "rb");
    if (!file) return 3;
    if (fseek(file, 0, SEEK_END) != 0) return 3;
    long size = ftell(file);
    if (size <= 0 || size % sizeof(float) || size / sizeof(float) > INT_MAX) return 3;
    rewind(file);
    float *pcm = malloc((size_t)size);
    if (!pcm || fread(pcm, 1, (size_t)size, file) != (size_t)size) return 3;
    fclose(file);

    struct transcribe_session *session = NULL;
    int status = transcribe_open(argv[1], NULL, NULL, &session);
    if (status == TRANSCRIBE_OK) {
        struct transcribe_run_params params;
        transcribe_run_params_init(&params);
        status = transcribe_run(session, pcm, (int)(size / sizeof(float)), &params);
        if (status == TRANSCRIBE_OK) puts(transcribe_full_text(session));
        transcribe_session_free(session);
    }
    free(pcm);
    if (status != TRANSCRIBE_OK) fprintf(stderr, "%s\n", transcribe_status_string(status));
    return status == TRANSCRIBE_OK ? 0 : 1;
}
