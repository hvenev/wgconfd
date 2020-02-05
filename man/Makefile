PAGES = wgconfd.5 wgconfd.8

.PHONY: all clean
all: $(PAGES)

clean:
	rm $(PAGES)

%: %.scd
	scdoc < $< > $@
