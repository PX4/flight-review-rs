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
}

export interface ListFilters {
  sys_name?: string;
  ver_hw?: string;
  search?: string;
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
