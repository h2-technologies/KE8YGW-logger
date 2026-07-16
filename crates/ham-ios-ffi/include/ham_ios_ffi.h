#ifndef HAM_IOS_FFI_H
#define HAM_IOS_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

uint32_t ham_ios_abi_version(void);

char *ham_ios_call_json_bytes(const uint8_t *ptr, size_t len);
char *ham_ios_call_json(const char *input);
void ham_ios_free_string(char *ptr);

char *ham_ios_version_json(void);
char *ham_ios_dashboard_snapshot_json(void);
char *ham_ios_station_book_json(void);
char *ham_ios_provider_status_json(void);
char *ham_ios_map_snapshot_json(void);
char *ham_ios_sync_snapshot_json(void);
char *ham_ios_diagnostics_json(void);
char *ham_ios_lookup_callsign_json(const char *callsign);
char *ham_ios_grid_info_json(const char *grid);
char *ham_ios_infer_band_json(uint64_t frequency_hz);
char *ham_ios_parse_adif_json(const char *input);
char *ham_ios_export_adif_json(const char *qsos_json);

#ifdef __cplusplus
}
#endif

#endif
