// --- Backend data types ---

export interface LogRecord {
  id: string;
  filename: string;
  created_at: string;
  file_size: number;
  sys_name: string | null;
  ver_hw: string | null;
  ver_sw_release_str: string | null;
  flight_duration_s: number | null;
  topic_count: number;
  lat: number | null;
  lon: number | null;
  is_public: boolean;
  description: string | null;
  wind_speed: string | null;
  rating: number | null;
  feedback: string | null;
  video_url: string | null;
  source: string | null;
  pilot_name: string | null;
  vehicle_name: string | null;
  tags: string | null;
  location_name: string | null;
  mission_type: string | null;
  vehicle_type: string | null;
}

export interface ListFilters {
  search?: string;
  sys_name?: string;
  ver_hw?: string;
  vehicle_type?: string;
  ver_sw_release_str?: string;
  location_name?: string;
  flight_duration_min?: number;
  flight_duration_max?: number;
  date_from?: string;
  date_to?: string;
  vibration_status?: string;
  has_gps?: boolean;
  tag?: string;
  sort?: string;
  page: number;
  limit: number;
}

export interface ListResponse {
  logs: LogRecord[];
  total: number;
}

export interface UploadOptions {
  description?: string;
  isPublic?: boolean;
  windSpeed?: string;
  rating?: number;
  feedback?: string;
  videoUrl?: string;
  source?: string;
  pilotName?: string;
  vehicleName?: string;
  tags?: string;
  locationName?: string;
  missionType?: string;
}

export interface UploadResponse {
  id: string;
  filename: string;
  sys_name: string | null;
  ver_hw: string | null;
  flight_duration_s: number | null;
  topic_count: number;
  is_public: boolean;
  delete_token: string;
  parquet_files: string[];
}

// --- Metadata types ---

export interface FlightMetadata {
  sys_name: string | null;
  ver_hw: string | null;
  ver_sw_release_str: string | null;
  sys_uuid: string | null;
  flight_duration_s: number | null;
  topics: Record<string, TopicInfo>;
  dropout_count: number;
  logged_messages: LogEntry[];
  tagged_logged_messages: TaggedLogEntry[];
  parameters: Record<string, number>;
  changed_parameters: ChangedParam[];
  default_parameters: Record<string, number>;
  analysis: FlightAnalysis | null;
  multi_info?: Record<string, string[]>;
}

export interface TopicInfo {
  message_count: number;
  multi_id: number;
}

export interface LogEntry {
  level: string;
  timestamp_us: number;
  message: string;
}

export interface TaggedLogEntry {
  level: string;
  tag: number;
  timestamp_us: number;
  message: string;
}

export interface ChangedParam {
  name: string;
  value: number;
  in_flight: boolean;
}

export interface FlightAnalysis {
  flight_modes: FlightModeSegment[];
  vtol_states: VtolStateSegment[];
  stats: FlightStats;
  battery: BatterySummary;
  gps_quality: GpsQuality;
  vibration: VibrationSummary;
  non_default_params: ParamDiff[];
  gps_track: TrackPoint[];
}

export interface FlightModeSegment {
  mode: string;
  mode_id: number;
  start_us: number;
  end_us: number;
  duration_s: number;
}

export interface VtolStateSegment {
  state: string;
  start_us: number;
  end_us: number;
}

export interface FlightStats {
  total_distance_m: number;
  max_altitude_diff_m: number;
  max_speed_m_s: number;
}

export interface BatterySummary {
  discharged_mah: number | null;
  min_voltage_v: number | null;
}

export interface GpsQuality {
  max_satellites: number | null;
  max_eph_m: number | null;
}

export interface VibrationSummary {
  accel_vibe_mean: number | null;
  status: string;
}

export interface ParamDiff {
  name: string;
  value: number;
  default: number;
}

export interface TrackPoint {
  lat_deg: number;
  lon_deg: number;
  alt_m: number;
  timestamp_us: number;
  mode_id: number;
}

// --- Stats types ---

export interface StatsFilters {
  vehicleType?: string;
  verHw?: string;
  source?: string;
}

export interface StatsResponse {
  group_by: string;
  period: string;
  data: StatsDataPoint[];
}

export interface StatsDataPoint {
  group: string;
  count: number;
  avg_flight_duration_s?: number;
  total_flight_hours?: number;
  avg_max_speed?: number;
}

export interface PlotConfig {
  id: string;
  topic: string;
  multiId: number;
  fields: string[];
  yLabel: string;
  colors: string[];
  minimized?: boolean;
  /** Plot rendering kind. 'timeseries' (default) plots fields vs time;
   *  'xy' is a special trajectory/scatter plot with hardcoded topics. */
  kind?: 'timeseries' | 'xy';
}
