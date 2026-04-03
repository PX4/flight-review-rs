import type { LogRecord, ListFilters, ListResponse, UploadOptions, UploadResponse, FlightMetadata, StatsResponse } from './types';

const BASE = '/api';

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = 'ApiError';
  }
}

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, init);
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new ApiError(res.status, body || res.statusText);
  }
  return res.json();
}

export async function getLog(id: string): Promise<LogRecord> {
  return apiFetch(`/logs/${id}`);
}

export async function listLogs(filters: ListFilters): Promise<ListResponse> {
  const params = new URLSearchParams();
  if (filters.search) params.set('search', filters.search);
  if (filters.sys_name) params.set('sys_name', filters.sys_name);
  if (filters.ver_hw) params.set('ver_hw', filters.ver_hw);
  if (filters.vehicle_type) params.set('vehicle_type', filters.vehicle_type);
  if (filters.ver_sw_release_str) params.set('ver_sw_release_str', filters.ver_sw_release_str);
  if (filters.location_name) params.set('location_name', filters.location_name);
  if (filters.flight_duration_min != null) params.set('flight_duration_min', String(filters.flight_duration_min));
  if (filters.flight_duration_max != null) params.set('flight_duration_max', String(filters.flight_duration_max));
  if (filters.date_from) params.set('date_from', filters.date_from);
  if (filters.date_to) params.set('date_to', filters.date_to);
  if (filters.vibration_status) params.set('vibration_status', filters.vibration_status);
  if (filters.has_gps != null) params.set('has_gps', String(filters.has_gps));
  if (filters.tag) params.set('tag', filters.tag);
  if (filters.sort) params.set('sort', filters.sort);
  params.set('offset', String((filters.page - 1) * filters.limit));
  params.set('limit', String(filters.limit));
  return apiFetch(`/logs?${params}`);
}

export interface FilterFacets {
  ver_hw: string[];
  vehicle_type: string[];
  ver_sw_release_str: string[];
  vibration_status: string[];
  tags: string[];
}

export async function getFilterFacets(): Promise<FilterFacets> {
  return apiFetch('/logs/facets');
}

export function uploadLog(
  file: File,
  opts: UploadOptions,
  onProgress?: (pct: number) => void
): { promise: Promise<UploadResponse>; abort: () => void } {
  const xhr = new XMLHttpRequest();
  const promise = new Promise<UploadResponse>((resolve, reject) => {
    const form = new FormData();
    form.append('file', file);
    if (opts.description) form.append('description', opts.description);
    if (opts.isPublic) form.append('is_public', 'true');
    if (opts.windSpeed) form.append('wind_speed', opts.windSpeed);
    if (opts.rating != null) form.append('rating', String(opts.rating));
    if (opts.feedback) form.append('feedback', opts.feedback);
    if (opts.videoUrl) form.append('video_url', opts.videoUrl);
    if (opts.source) form.append('source', opts.source);
    if (opts.pilotName) form.append('pilot_name', opts.pilotName);
    if (opts.vehicleName) form.append('vehicle_name', opts.vehicleName);
    if (opts.tags) form.append('tags', opts.tags);
    if (opts.locationName) form.append('location_name', opts.locationName);
    if (opts.missionType) form.append('mission_type', opts.missionType);

    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable && onProgress) onProgress((e.loaded / e.total) * 100);
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        resolve(JSON.parse(xhr.responseText));
      } else {
        reject(new ApiError(xhr.status, xhr.responseText || xhr.statusText));
      }
    };
    xhr.onerror = () => reject(new Error('Network error'));
    xhr.open('POST', `${BASE}/upload`);
    xhr.send(form);
  });
  return { promise, abort: () => xhr.abort() };
}

export async function getMetadata(id: string): Promise<FlightMetadata> {
  return apiFetch(`/logs/${id}/data/metadata.json`);
}

export interface TrackPointCompact {
  lat: number;
  lon: number;
  m: number; // mode_id
}

export async function getTrack(id: string): Promise<TrackPointCompact[]> {
  return apiFetch(`/logs/${id}/track`);
}

export async function deleteLog(id: string, token: string): Promise<void> {
  const res = await fetch(`${BASE}/logs/${id}?token=${token}`, { method: 'DELETE' });
  if (!res.ok) throw new ApiError(res.status, await res.text());
}

export async function getStats(params: {
  group_by: string;
  period?: string;
  limit?: number;
  vehicle_type?: string;
  ver_hw?: string;
  source?: string;
}): Promise<StatsResponse> {
  const p = new URLSearchParams();
  p.set('group_by', params.group_by);
  if (params.period) p.set('period', params.period);
  if (params.limit) p.set('limit', String(params.limit));
  if (params.vehicle_type) p.set('vehicle_type', params.vehicle_type);
  if (params.ver_hw) p.set('ver_hw', params.ver_hw);
  if (params.source) p.set('source', params.source);
  return apiFetch(`/stats?${p}`);
}
