<script lang="ts" module>
	export interface AvatarProps {
		src: string;
		alt?: string | null;
	}
</script>

<script lang="ts">
	import { getAverageLuminance } from '$lib/common/util/canvas';

	let { src, alt }: AvatarProps = $props();

	let imgElement = $state<HTMLImageElement | null>();
	let isDark = $state(true);

	$effect(() => {
		if (imgElement) {
			getAverageLuminance(imgElement).then((luminance) => {
				isDark = luminance < 200;
			});
		}
	});
</script>

<img
	bind:this={imgElement}
	{src}
	{alt}
	crossorigin="anonymous"
	class={{
		'h-24 w-24 rounded-full p-4': true,
		'bg-black': !isDark,
		'bg-white': isDark
	}}
/>
