import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	IMAGE_CROP_MAX_BYTES,
	IMAGE_CROP_MAX_PIXELS,
	clampZoom,
	cropGeometry,
	panCrop,
	validateImageDimensions,
	validateImageFile,
	zoomCrop,
} from '../src/lib/image-crop.ts';

const viewport = { width: 300, height: 300 };

test('validates supported image files and the byte budget', () => {
	assert.equal(validateImageFile({ name: 'a.PNG', size: 10, type: 'image/png' }), null);
	assert.equal(validateImageFile({ name: 'a.jpg', size: 10, type: 'image/jpeg' }), null);
	assert.equal(validateImageFile({ name: 'a.webp', size: 10, type: '' }), null);
	assert.equal(validateImageFile({ name: 'a.gif', size: 10, type: 'image/gif' }), 'unsupported');
	assert.equal(validateImageFile({ name: 'a.png', size: 0, type: 'image/png' }), 'tooLarge');
	assert.equal(validateImageFile({ name: 'a.png', size: IMAGE_CROP_MAX_BYTES + 1, type: 'image/png' }), 'tooLarge');
});

test('validates pixel and dimension budgets', () => {
	assert.equal(validateImageDimensions(1600, 1000), null);
	assert.equal(validateImageDimensions(4001, 4000), 'tooManyPixels');
	assert.equal(validateImageDimensions(16001, 1), 'tooLargeDimension');
	assert.equal(validateImageDimensions(0, 1), 'tooLargeDimension');
	assert.equal(IMAGE_CROP_MAX_PIXELS, 16_000_000);
});

test('cover-fits a wide image and centers its source crop', () => {
	const geometry = cropGeometry({ width: 800, height: 400 }, viewport, { zoom: 1, offsetX: 0, offsetY: 0 });
	assert.equal(geometry.scale, 0.75);
	assert.equal(geometry.displayWidth, 600);
	assert.equal(geometry.displayHeight, 300);
	assert.equal(geometry.maxOffsetX, 150);
	assert.equal(geometry.maxOffsetY, 0);
	assert.equal(geometry.sourceSize, 400);
	assert.equal(geometry.sourceX, 200);
	assert.equal(geometry.sourceY, 0);
});

test('clamps panning to the image edges and keeps the crop in bounds', () => {
	const image = { width: 800, height: 400 };
	const panned = panCrop(image, viewport, { zoom: 1, offsetX: 0, offsetY: 0 }, 1000, 1000);
	const geometry = cropGeometry(image, viewport, panned);
	assert.equal(geometry.offsetX, 150);
	assert.equal(geometry.offsetY, 0);
	assert.equal(geometry.sourceX, 0);
	assert.equal(geometry.sourceY, 0);
	const otherSide = cropGeometry(image, viewport, panCrop(image, viewport, { zoom: 1, offsetX: 0, offsetY: 0 }, -1000, -1000));
	assert.equal(otherSide.offsetX, -150);
	assert.equal(otherSide.sourceX, 400);
});

test('zooming tightens the source crop and clamps to the supported range', () => {
	const image = { width: 800, height: 400 };
	const geometry = cropGeometry(image, viewport, { zoom: 2, offsetX: 0, offsetY: 0 });
	assert.equal(geometry.sourceSize, 200);
	assert.equal(geometry.sourceX, 300);
	assert.equal(clampZoom(0), 1);
	assert.equal(clampZoom(99), 4);
	assert.equal(zoomCrop(image, viewport, { zoom: 1, offsetX: 150, offsetY: 0 }, 2).offsetX, 150);
	assert.equal(zoomCrop(image, viewport, { zoom: 1, offsetX: 1000, offsetY: 0 }, 2).offsetX, 450);
});

test('portrait images fill the viewport without exposing empty edges', () => {
	const geometry = cropGeometry({ width: 400, height: 800 }, { width: 300, height: 300 }, { zoom: 1, offsetX: 0, offsetY: 0 });
	assert.equal(geometry.displayWidth, 300);
	assert.equal(geometry.displayHeight, 600);
	assert.equal(geometry.maxOffsetX, 0);
	assert.equal(geometry.maxOffsetY, 150);
	assert.equal(geometry.sourceSize, 400);
	assert.equal(geometry.sourceX, 0);
	assert.equal(geometry.sourceY, 200);
});
