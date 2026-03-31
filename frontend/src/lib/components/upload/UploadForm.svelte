<script lang="ts">
	import type { UploadOptions } from '$lib/types';
	import { formatFileSize } from '$lib/utils/formatters';

	let { file, onSubmit, disabled = false } = $props<{
		file: File;
		onSubmit: (opts: UploadOptions) => void;
		disabled: boolean;
	}>();

	let description = $state('');
	let windSpeed = $state('');
	let rating = $state('');
	let pilotName = $state('');
	let vehicleName = $state('');
	let locationName = $state('');
	let tags = $state('');
	let isPublic = $state(false);

	function handleSubmit(e: Event) {
		e.preventDefault();
		const opts: UploadOptions = {};
		if (description) opts.description = description;
		if (windSpeed) opts.windSpeed = windSpeed;
		if (rating) opts.rating = Number(rating);
		if (pilotName) opts.pilotName = pilotName;
		if (vehicleName) opts.vehicleName = vehicleName;
		if (locationName) opts.locationName = locationName;
		if (tags) opts.tags = tags;
		opts.isPublic = isPublic;
		onSubmit(opts);
	}
</script>

<div class="rounded-lg bg-white ring-1 ring-gray-200 shadow-sm p-6 mb-8">
	<!-- Selected file indicator -->
	<div class="flex items-center gap-3 rounded-md bg-emerald-50 border border-emerald-200 px-4 py-3 mb-6">
		<svg class="size-5 text-emerald-600" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
			<path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
		</svg>
		<span class="text-sm font-medium text-emerald-600">{file.name}</span>
		<span class="text-sm text-gray-500 ml-auto">{formatFileSize(file.size)}</span>
	</div>

	<!-- Form fields -->
	<form onsubmit={handleSubmit} class="space-y-5">
		<div>
			<label for="description" class="block text-sm font-medium text-gray-700 mb-1.5">Description</label>
			<textarea
				id="description"
				rows={3}
				placeholder="Describe your flight..."
				bind:value={description}
				disabled={disabled}
				class="block w-full rounded-md bg-white px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none disabled:opacity-50"
			></textarea>
		</div>

		<div class="grid grid-cols-2 gap-4">
			<div>
				<label for="wind-speed" class="block text-sm font-medium text-gray-700 mb-1.5">Wind Speed</label>
				<select
					id="wind-speed"
					bind:value={windSpeed}
					disabled={disabled}
					class="block w-full rounded-md bg-white px-3 py-2 text-sm text-gray-900 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none disabled:opacity-50"
				>
					<option value="">Select...</option>
					<option value="calm">Calm</option>
					<option value="breeze">Breeze</option>
					<option value="gale">Gale</option>
					<option value="storm">Storm</option>
				</select>
			</div>
			<div>
				<label for="rating" class="block text-sm font-medium text-gray-700 mb-1.5">Rating</label>
				<select
					id="rating"
					bind:value={rating}
					disabled={disabled}
					class="block w-full rounded-md bg-white px-3 py-2 text-sm text-gray-900 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none disabled:opacity-50"
				>
					<option value="">Select...</option>
					<option value="1">1</option>
					<option value="2">2</option>
					<option value="3">3</option>
					<option value="4">4</option>
					<option value="5">5</option>
				</select>
			</div>
		</div>

		<div class="grid grid-cols-2 gap-4">
			<div>
				<label for="pilot-name" class="block text-sm font-medium text-gray-700 mb-1.5">Pilot Name</label>
				<input
					id="pilot-name"
					type="text"
					placeholder="Your name"
					bind:value={pilotName}
					disabled={disabled}
					class="block w-full rounded-md bg-white px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none disabled:opacity-50"
				/>
			</div>
			<div>
				<label for="vehicle-name" class="block text-sm font-medium text-gray-700 mb-1.5">Vehicle Name</label>
				<input
					id="vehicle-name"
					type="text"
					placeholder="e.g. My Quad"
					bind:value={vehicleName}
					disabled={disabled}
					class="block w-full rounded-md bg-white px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none disabled:opacity-50"
				/>
			</div>
		</div>

		<div class="grid grid-cols-2 gap-4">
			<div>
				<label for="location" class="block text-sm font-medium text-gray-700 mb-1.5">Location</label>
				<input
					id="location"
					type="text"
					placeholder="e.g. Test field, Zurich"
					bind:value={locationName}
					disabled={disabled}
					class="block w-full rounded-md bg-white px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none disabled:opacity-50"
				/>
			</div>
			<div>
				<label for="tags" class="block text-sm font-medium text-gray-700 mb-1.5">Tags</label>
				<input
					id="tags"
					type="text"
					placeholder="Comma-separated"
					bind:value={tags}
					disabled={disabled}
					class="block w-full rounded-md bg-white px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 ring-1 ring-gray-300 focus:ring-2 focus:ring-indigo-500 outline-none disabled:opacity-50"
				/>
			</div>
		</div>

		<div class="flex items-center justify-between pt-2">
			<label class="flex items-center gap-2 cursor-pointer">
				<input
					type="checkbox"
					bind:checked={isPublic}
					disabled={disabled}
					class="size-4 rounded border-gray-300 bg-white text-indigo-500 focus:ring-indigo-500"
				/>
				<span class="text-sm text-gray-600">Make this log public</span>
			</label>
			<button
				type="submit"
				disabled={disabled}
				class="rounded-md bg-indigo-500 px-4 py-2 text-sm font-semibold text-white hover:bg-indigo-400 focus:outline-2 focus:outline-offset-2 focus:outline-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed"
			>
				Upload
			</button>
		</div>
	</form>
</div>
