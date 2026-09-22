export function ImageStep({ step, image }: { step: string; image: string }) {
    return (
        <div className="py-4">
            <p>{step}</p>
            <img src={image} alt="Nikon Connection Help" className="max-h-84 mt-2" />
        </div>
    );
}

export function TextStep({ step }: { step: string }) {
    return (
        <div className="py-4">
            <p>{step}</p>
        </div>
    );
}
