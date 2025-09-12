import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center rounded-md text-sm font-medium transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 relative overflow-hidden",
  {
    variants: {
      variant: {
        default:
          "bg-[hsl(186,90%,61%)] text-[#0c0c0f] hover:bg-[hsl(186,90%,55%)] glow glow-cyan",
        destructive:
          "bg-destructive text-destructive-foreground hover:bg-destructive/90",
        outline:
          "border border-input bg-background hover:bg-accent hover:text-accent-foreground",
        secondary:
          "bg-[hsl(157,74%,67%)] text-[#0c0c0f] hover:bg-[hsl(157,74%,60%)] glow glow-green",
        ghost: "hover:bg-accent/20 hover:text-accent-foreground",
        link: "text-[hsl(186,90%,61%)] underline-offset-4 hover:underline glow-text-cyan",
      },
      size: {
        default: "h-10 px-4 py-2",
        sm: "h-9 rounded-md px-3",
        lg: "h-11 rounded-md px-8",
        icon: "h-10 w-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, onClick, ...props }, ref) => {
    // Enhanced click handler to prevent common issues
    const handleClick = React.useCallback(
      (event: React.MouseEvent<HTMLButtonElement>) => {
        // Prevent event bubbling issues
        if (event.currentTarget === event.target) {
          event.stopPropagation();
        }

        // Explicitly focus the button to improve accessibility
        event.currentTarget.focus();

        // Call the original onClick handler if provided
        if (onClick) {
          onClick(event);
        }
      },
      [onClick],
    );

    const Comp = asChild ? React.Fragment : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        onClick={handleClick}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";

export { Button, buttonVariants };
