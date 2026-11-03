export const IMAGE_CROP_MAX_BYTES = 8 * 1024 * 1024;
export const IMAGE_CROP_MAX_PIXELS = 16_000_000;
export const IMAGE_CROP_MAX_DIMENSION = 16_000;
export const IMAGE_CROP_MIN_ZOOM = 1;
export const IMAGE_CROP_MAX_ZOOM = 4;
export const IMAGE_CROP_OUTPUT_SIZE = 512;

export interface ImageSize {
	width: number;
	height: number;
}
export interface CropState {
	zoom: number;
	offsetX: number;
	offsetY: number;
}
export interface CropGeometry {
	scale: number;
	displayWidth: number;
	displayHeight: number;
	maxOffsetX: number;
	maxOffsetY: number;
	offsetX: number;
	offsetY: number;
	sourceX: number;
	sourceY: number;
	sourceSize: number;
}
export type ImageFileError = 'unsupported' | 'tooLarge' | null;
export type ImageDimensionError = 'tooManyPixels' | 'tooLargeDimension' | null;

const supportedTypes = new Set(['image/png', 'image/jpeg', 'image/webp']);
const supportedExtensions = new Set(['png', 'jpg', 'jpeg', 'webp']);

export function validateImageFile(file: Pick<File, 'name' | 'size' | 'type'>): ImageFileError {
	const extension = file.name.toLowerCase().split('.').pop() ?? '';
	if (!supportedTypes.has(file.type.toLowerCase()) && !supportedExtensions.has(extension)) return 'unsupported';
	if (file.size <= 0 || file.size > IMAGE_CROP_MAX_BYTES) return 'tooLarge';
	return null;
}

export function validateImageDimensions(width: number, height: number): ImageDimensionError {
	if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return 'tooLargeDimension';
	if (width > IMAGE_CROP_MAX_DIMENSION || height > IMAGE_CROP_MAX_DIMENSION) return 'tooLargeDimension';
	if (width * height > IMAGE_CROP_MAX_PIXELS) return 'tooManyPixels';
	return null;
}

export function clampZoom(value: number): number {
	if (!Number.isFinite(value)) return IMAGE_CROP_MIN_ZOOM;
	return Math.min(IMAGE_CROP_MAX_ZOOM, Math.max(IMAGE_CROP_MIN_ZOOM, value));
}

/** Cover-fit geometry in viewport pixels, with translation clamped to the image edges. */
export function cropGeometry(image: ImageSize, viewport: ImageSize, state: CropState): CropGeometry {
	const imageWidth = Math.max(1, image.width);
	const imageHeight = Math.max(1, image.height);
	const viewportWidth = Math.max(1, viewport.width);
	const viewportHeight = Math.max(1, viewport.height);
	const scale = Math.max(viewportWidth / imageWidth, viewportHeight / imageHeight) * clampZoom(state.zoom);
	const displayWidth = imageWidth * scale;
	const displayHeight = imageHeight * scale;
	const maxOffsetX = Math.max(0, (displayWidth - viewportWidth) / 2);
	const maxOffsetY = Math.max(0, (displayHeight - viewportHeight) / 2);
	const offsetX = Math.min(maxOffsetX, Math.max(-maxOffsetX, Number.isFinite(state.offsetX) ? state.offsetX : 0));
	const offsetY = Math.min(maxOffsetY, Math.max(-maxOffsetY, Number.isFinite(state.offsetY) ? state.offsetY : 0));
	// The viewport sees the source region opposite the image translation.
	const sourceSize = Math.min(viewportWidth, viewportHeight) / scale;
	const sourceX = Math.min(
		Math.max(0, imageWidth - sourceSize),
		Math.max(0, imageWidth / 2 - offsetX / scale - sourceSize / 2),
	);
	const sourceY = Math.min(
		Math.max(0, imageHeight - sourceSize),
		Math.max(0, imageHeight / 2 - offsetY / scale - sourceSize / 2),
	);
	return { scale, displayWidth, displayHeight, maxOffsetX, maxOffsetY, offsetX, offsetY, sourceX, sourceY, sourceSize };
}

export function panCrop(
	image: ImageSize,
	viewport: ImageSize,
	state: CropState,
	deltaX: number,
	deltaY: number,
): CropState {
	const geometry = cropGeometry(image, viewport, state);
	return {
		zoom: clampZoom(state.zoom),
		offsetX: geometry.offsetX + deltaX,
		offsetY: geometry.offsetY + deltaY,
	};
}

export function zoomCrop(
	image: ImageSize,
	viewport: ImageSize,
	state: CropState,
	zoom: number,
): CropState {
	const geometry = cropGeometry(image, viewport, { ...state, zoom: clampZoom(zoom) });
	return { zoom: clampZoom(zoom), offsetX: geometry.offsetX, offsetY: geometry.offsetY };
}
