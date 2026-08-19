#pragma once

#include <stdint.h>

int pocket_shm_open_readwrite(const char *name);
void pocket_frame_publish_begin(void *mapping);
void pocket_frame_publish_end(void *mapping, uint64_t sequence);
