CORE_DIR := ../tv7-core
ifeq ("$(wildcard $(CORE_DIR))","")
$(error tv7-core not found. Please clone it as a sibling directory: ../tv7-core)
endif

-include Makefile.core
