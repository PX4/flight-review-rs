/**
 * Parameter file export utilities.
 *
 * Supports the QGroundControl `.params` format (tab-separated), which can be
 * loaded back into QGC or MAVLink tools to replicate a vehicle's configuration.
 *
 * Format (one header block, then one row per parameter):
 *   # Onboard parameters for Vehicle <id>
 *   #
 *   # Vehicle-Id Component-Id Name Value Type
 *   1\t1\tPARAM_NAME\t<value>\t<type>
 *
 * Type codes follow MAV_PARAM_TYPE. We emit 9 (REAL32) for all values since the
 * backend exposes parameters as plain numbers without type discrimination — QGC
 * accepts this on import.
 */

const MAV_PARAM_TYPE_REAL32 = 9;

export interface ParamFileOptions {
  sysName?: string | null;
  vehicleId?: number;
  componentId?: number;
}

/** Format a single param value for the QGC params file. */
function formatValue(value: number): string {
  if (Number.isInteger(value)) return value.toFixed(1);
  // Use enough precision to round-trip float32 values.
  return value.toPrecision(9).replace(/0+$/, '').replace(/\.$/, '.0');
}

/** Build the contents of a QGC `.params` file from a parameter dictionary. */
export function buildQgcParamsFile(
  parameters: Record<string, number>,
  options: ParamFileOptions = {}
): string {
  const { sysName, vehicleId = 1, componentId = 1 } = options;
  const names = Object.keys(parameters).sort();

  const header = [
    `# Onboard parameters for Vehicle ${sysName ?? vehicleId}`,
    '#',
    '# Vehicle-Id Component-Id Name Value Type',
  ];

  const rows = names.map((name) =>
    [vehicleId, componentId, name, formatValue(parameters[name]), MAV_PARAM_TYPE_REAL32].join('\t')
  );

  return header.concat(rows).join('\n') + '\n';
}

/** Trigger a browser download of the given text content. */
export function downloadTextFile(filename: string, content: string, mime = 'text/plain'): void {
  const blob = new Blob([content], { type: `${mime};charset=utf-8` });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
